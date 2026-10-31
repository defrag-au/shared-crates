//! The flow, driven end to end with scripted replies.
//!
//! The addresses are the ones `wallet-link`'s own tests use; the replies are
//! written in auth-service's shapes (`social_link.rs`).

use super::*;

const STAKE: &str = "stake1u8962x3wtddcq2syq258ka3d9mxxkx5md5xawzx67pac9tgc5rhq9";
const EVM: &str = "0x7980aa64093853cb78c927e05b88fed96e945f81";
const USER: &str = "179744071361757184";
const MESSAGE: &str = "Link this wallet to your Discord account.\n\nDiscord ID: 179744071361757184\nNonce: ab12";

fn ok(call: CallKind, body: &str) -> Event {
    Event::Answer(Answer {
        call,
        reply: Reply::Ok(body.into()),
    })
}

fn status(call: CallKind, status: u16, body: &str) -> Event {
    Event::Answer(Answer {
        call,
        reply: Reply::Status {
            status,
            body: body.into(),
        },
    })
}

fn me() -> String {
    format!(r#"{{"discord_user_id":"{USER}","intent":"link_wallet"}}"#)
}

fn challenge() -> String {
    serde_json::to_string(&wallet_link::link::LinkChallenge {
        nonce: "ab12".into(),
        message: MESSAGE.into(),
        expires: 1_761_900_000,
    })
    .unwrap()
}

fn evm_linked() -> String {
    format!(r#"[{{"chain":"eip155","address":"{EVM}","handle":null}}]"#)
}

/// Loaded, with the EVM account already linked.
fn loaded() -> Linker {
    let mut link = Linker::new();
    let effects = link.handle(Event::Start);
    assert_eq!(
        effects,
        [Effect::Call(ApiCall::Me), Effect::Call(ApiCall::Wallets)]
    );
    assert_eq!(link.phase(), Phase::Loading);
    link.handle(ok(CallKind::Wallets, &evm_linked()));
    assert_eq!(link.phase(), Phase::Loading, "both replies are needed");
    link.handle(ok(CallKind::Me, &me()));
    assert_eq!(link.phase(), Phase::Idle);
    assert_eq!(link.identity().unwrap().discord_user_id, USER);
    assert_eq!(link.wallets().unwrap().len(), 1);
    link
}

fn cardano() -> Claim {
    Claim::Cardano {
        stake_address: STAKE.into(),
    }
}

/// Connected and signing a Cardano link.
fn signing() -> Linker {
    let mut link = loaded();
    assert_eq!(
        link.handle(Event::Link(None)),
        [Effect::OpenConnector(None)]
    );
    assert_eq!(
        link.handle(Event::Claim(cardano())),
        [Effect::Call(ApiCall::Challenge(ChainFamily::Cardano))]
    );
    let effects = link.handle(ok(CallKind::Challenge, &challenge()));
    let [Effect::Sign(SignRequest::Cardano {
        address_hex,
        payload_hex,
    })] = effects.as_slice()
    else {
        panic!("a Cardano signature asked for: {effects:?}");
    };
    assert_eq!(address_hex, &bech32_to_hex(STAKE).unwrap());
    assert_eq!(payload_hex, &hex(MESSAGE.as_bytes()));
    assert_eq!(
        link.phase(),
        Phase::Link {
            family: Some(ChainFamily::Cardano),
            step: LinkStep::Sign
        }
    );
    link
}

fn signed() -> Event {
    Event::Signed(Signature::Cardano {
        signature: "84a4".into(),
        key: "a401".into(),
    })
}

#[test]
fn a_cardano_link_runs_challenge_sign_verify_and_lands_in_the_list() {
    let mut link = signing();
    assert_eq!(
        link.handle(signed()),
        [Effect::Call(ApiCall::VerifyCardano(CardanoLinkRequest {
            stake_address: STAKE.into(),
            signature: "84a4".into(),
            public_key: "a401".into(),
        }))]
    );
    let verified = format!(
        r#"{{"chain":"cardano:mainnet","address":"{STAKE}","handle":"boef","discord_user_id":"{USER}"}}"#
    );
    let effects = link.handle(ok(CallKind::Verify, &verified));
    let wallet = LinkedWallet {
        chain: "cardano:mainnet".into(),
        address: STAKE.into(),
        handle: Some("boef".into()),
    };
    assert_eq!(
        effects,
        [
            Effect::Linked(wallet.clone()),
            Effect::Call(ApiCall::Wallets)
        ]
    );
    assert_eq!(
        link.phase(),
        Phase::Link {
            family: Some(ChainFamily::Cardano),
            step: LinkStep::Linked {
                address: STAKE.into()
            }
        }
    );
    assert_eq!(link.wallets().unwrap().last(), Some(&wallet));
    link.handle(Event::Done);
    assert_eq!(link.phase(), Phase::Idle);
}

#[test]
fn a_wallet_already_linked_signs_nothing() {
    let mut link = loaded();
    link.handle(Event::Link(None));
    let effects = link.handle(Event::Claim(Claim::Evm {
        address: EVM.to_uppercase().replace("0X", "0x"),
    }));
    assert!(effects.is_empty(), "{effects:?}");
    assert!(matches!(
        link.phase(),
        Phase::Link {
            family: Some(ChainFamily::Evm),
            step: LinkStep::Already { .. }
        }
    ));
}

#[test]
fn a_refusal_is_kept_and_a_retry_asks_for_a_fresh_challenge() {
    let mut link = signing();
    assert!(link.handle(Event::SignRefused).is_empty());
    assert_eq!(
        link.phase(),
        Phase::Link {
            family: Some(ChainFamily::Cardano),
            step: LinkStep::Declined
        }
    );
    assert_eq!(
        link.handle(Event::Retry),
        [Effect::Call(ApiCall::Challenge(ChainFamily::Cardano))],
        "the old challenge was burned"
    );
}

#[test]
fn a_wallet_linked_elsewhere_stops_with_the_servers_reason() {
    let mut link = signing();
    link.handle(signed());
    link.handle(status(CallKind::Verify, 409, "already linked"));
    assert_eq!(
        link.phase(),
        Phase::Link {
            family: Some(ChainFamily::Cardano),
            step: LinkStep::Failed(
                "That wallet is already linked to another Discord account.".into()
            )
        }
    );
}

#[test]
fn closing_the_connector_abandons_the_link_and_drops_late_answers() {
    let mut link = signing();
    link.handle(Event::ConnectorClosed);
    assert_eq!(link.phase(), Phase::Idle);
    assert!(link.handle(signed()).is_empty());
    assert!(
        link.handle(ok(CallKind::Verify, "{}")).is_empty(),
        "nothing waits on it"
    );
    assert_eq!(link.phase(), Phase::Idle);
}

#[test]
fn a_refused_token_ends_the_session_wherever_it_is_noticed() {
    let mut link = signing();
    link.handle(signed());
    link.handle(status(CallKind::Verify, 401, "bad token"));
    assert_eq!(link.phase(), Phase::Expired);
    assert!(link.handle(Event::Link(None)).is_empty());

    let mut early = Linker::new();
    early.handle(Event::Start);
    early.handle(status(CallKind::Me, 401, ""));
    assert_eq!(early.phase(), Phase::Expired);
}

#[test]
fn an_unreachable_service_can_be_retried() {
    let mut link = Linker::new();
    link.handle(Event::Start);
    link.handle(Event::Answer(Answer {
        call: CallKind::Me,
        reply: Reply::Offline("dns".into()),
    }));
    assert_eq!(link.phase(), Phase::Unreachable(words::OFFLINE.into()));
    assert_eq!(
        link.handle(Event::Retry),
        [Effect::Call(ApiCall::Me), Effect::Call(ApiCall::Wallets)]
    );
    assert_eq!(link.phase(), Phase::Loading);
}

#[test]
fn an_unlink_is_confirmed_then_the_servers_list_is_taken() {
    let mut link = loaded();
    assert!(link.handle(Event::AskUnlink("not linked".into())).is_empty());
    assert_eq!(link.phase(), Phase::Idle, "only a linked address asks");
    link.handle(Event::AskUnlink(EVM.into()));
    let wallet = link.wallets().unwrap()[0].clone();
    assert_eq!(link.phase(), Phase::Confirming(wallet.clone()));
    assert_eq!(
        link.handle(Event::ConfirmUnlink),
        [Effect::Call(ApiCall::Unlink(UnlinkRequest {
            address: EVM.into()
        }))]
    );
    assert_eq!(
        link.handle(ok(CallKind::Unlink, "[]")),
        [Effect::Unlinked(wallet)]
    );
    assert_eq!(link.wallets(), Some(&[][..]));
    assert_eq!(link.phase(), Phase::Idle);
}

#[test]
fn a_failed_unlink_keeps_the_wallet_and_says_why() {
    let mut link = loaded();
    link.handle(Event::AskUnlink(EVM.into()));
    link.handle(Event::ConfirmUnlink);
    let effects = link.handle(status(CallKind::Unlink, 500, ""));
    assert!(matches!(effects.as_slice(), [Effect::Notice(_)]), "{effects:?}");
    assert_eq!(link.wallets().unwrap().len(), 1);
    assert_eq!(link.phase(), Phase::Idle);
}

#[test]
fn calls_are_written_as_auth_service_routes_them() {
    assert_eq!(ApiCall::Me.path(), "/auth/me");
    assert_eq!(ApiCall::Wallets.method(), "GET");
    assert_eq!(
        ApiCall::Challenge(ChainFamily::Evm).path(),
        "/auth/evm/challenge"
    );
    assert_eq!(
        ApiCall::Challenge(ChainFamily::Cardano).path(),
        "/auth/cardano/challenge"
    );
    assert_eq!(ApiCall::Challenge(ChainFamily::Cardano).body(), None);
    let unlink = ApiCall::Unlink(UnlinkRequest {
        address: EVM.into(),
    });
    assert_eq!(unlink.method(), "POST");
    assert_eq!(unlink.path(), "/auth/wallets/unlink");
    assert_eq!(unlink.body().unwrap(), format!(r#"{{"address":"{EVM}"}}"#));
}

#[test]
fn a_link_names_its_family_by_namespace() {
    let w = |chain: &str| LinkedWallet {
        chain: chain.into(),
        address: String::new(),
        handle: None,
    };
    assert_eq!(family_of(&w("cardano:mainnet")), Some(ChainFamily::Cardano));
    assert_eq!(family_of(&w("eip155")), Some(ChainFamily::Evm));
    assert_eq!(family_of(&w("eip155:4663")), Some(ChainFamily::Evm));
    assert_eq!(family_of(&w("nope")), None);
}
