use super::*;

/// Two materials: bolts and gears.
type T = Tree<&'static str, 2>;

const fn node(
    id: &'static str,
    tier: u8,
    scope: Scope,
    cost: [u32; 2],
    needs: &'static [Need],
) -> Node<&'static str, 2> {
    Node {
        id,
        name: id,
        tier,
        scope,
        cost,
        needs,
        makes: id,
    }
}

/// A small tree: a recipe, research that unlocks a second recipe, a goal,
/// and a recipe needing both the goal and a score.
static NODES: [Node<&str, 2>; 5] = [
    node("tomatoes", 1, Scope::Recipe, [3, 0], &[]),
    node("toolkit", 1, Scope::Research, [4, 1], &[]),
    node("skates", 2, Scope::Recipe, [2, 2], &[Need::Node("toolkit")]),
    node(
        "generator",
        2,
        Scope::Goal,
        [100, 50],
        &[Need::Node("toolkit")],
    ),
    node(
        "engine",
        3,
        Scope::Recipe,
        [4, 3],
        &[Need::Node("generator"), Need::Score(60)],
    ),
];

fn tree() -> T {
    Tree::new(&NODES).expect("a tree")
}

fn view(have: [u32; 2], toward: &[[u32; 2]]) -> View<'_, 2> {
    View {
        mine: Unlocks::NONE,
        server: Unlocks::NONE,
        score: 0,
        have,
        toward,
    }
}

#[test]
fn a_good_tree_resolves_its_needs_and_tiers() {
    let t = tree();
    assert_eq!(t.len(), 5);
    assert_eq!(t.index("skates"), Some(2));
    assert_eq!(t.needs_of(4), [3]);
    assert_eq!(t.tiers(), [1, 2, 3]);
    assert_eq!(t.in_tier(2).collect::<Vec<_>>(), [2, 3]);
    let edges: Vec<_> = t.edges().collect();
    assert_eq!(edges, [(1, 2), (1, 3), (3, 4)]);
}

#[test]
fn a_bad_tree_is_refused_naming_the_node() {
    static DUP: [Node<&str, 2>; 2] = [
        node("a", 1, Scope::Recipe, [1, 0], &[]),
        node("a", 1, Scope::Recipe, [1, 0], &[]),
    ];
    assert_eq!(T::new(&DUP).unwrap_err(), TreeError::Duplicate("a"));

    static UNKNOWN: [Node<&str, 2>; 1] = [node("a", 1, Scope::Recipe, [1, 0], &[Need::Node("b")])];
    assert_eq!(T::new(&UNKNOWN).unwrap_err(), TreeError::Unknown("a", "b"));

    static LOOP: [Node<&str, 2>; 3] = [
        node("a", 1, Scope::Recipe, [1, 0], &[Need::Node("c")]),
        node("b", 1, Scope::Recipe, [1, 0], &[Need::Node("a")]),
        node("c", 1, Scope::Recipe, [1, 0], &[Need::Node("b")]),
    ];
    assert!(matches!(T::new(&LOOP).unwrap_err(), TreeError::Cycle(_)));

    static SELF: [Node<&str, 2>; 1] = [node("a", 1, Scope::Recipe, [1, 0], &[Need::Node("a")])];
    assert_eq!(T::new(&SELF).unwrap_err(), TreeError::Cycle("a"));

    static BAD_ID: [Node<&str, 2>; 1] = [node("Big Gun", 1, Scope::Recipe, [1, 0], &[])];
    assert_eq!(T::new(&BAD_ID).unwrap_err(), TreeError::BadId("Big Gun"));

    static EMPTY_GOAL: [Node<&str, 2>; 1] = [node("g", 1, Scope::Goal, [0, 0], &[])];
    assert_eq!(T::new(&EMPTY_GOAL).unwrap_err(), TreeError::EmptyGoal("g"));
}

#[test]
fn a_recipe_is_ready_when_carried_and_short_by_what_is_missing() {
    let t = tree();
    assert_eq!(t.status(0, &view([3, 0], &[])), Status::Ready);
    assert_eq!(
        t.status(0, &view([1, 9], &[])),
        Status::Short { short: [2, 0] }
    );
    // Built again and again; only the first build unlocks it.
    let first = t.plan(0, &view([3, 0], &[])).expect("ready");
    assert_eq!(first.take, [3, 0]);
    assert!(first.unlocks_mine);
    let mut again = view([6, 0], &[]);
    again.mine.set(0);
    assert!(!t.plan(0, &again).expect("ready").unlocks_mine);
}

#[test]
fn research_is_done_once_and_opens_what_needs_it() {
    let t = tree();
    let mut v = view([9, 9], &[]);
    assert_eq!(
        t.status(2, &v),
        Status::Locked {
            needs: vec![Need::Node("toolkit")]
        }
    );
    assert!(t.plan(1, &v).expect("ready").unlocks_mine);
    v.mine.set(1);
    assert_eq!(t.status(1, &v), Status::Done);
    assert_eq!(t.plan(1, &v), None, "research is done once");
    assert_eq!(t.status(2, &v), Status::Ready, "skates are open");
}

#[test]
fn a_goal_takes_what_it_still_wants_and_fills_for_everyone() {
    let t = tree();
    let toward = [[0, 0], [0, 0], [0, 0], [95, 50], [0, 0]];
    let mut v = view([10, 4], &toward);
    v.mine.set(1);
    // It wants 5 more bolts and no gears: only those are taken.
    let plan = t.plan(3, &v).expect("ready");
    assert_eq!(plan.take, [5, 0]);
    assert!(plan.fills_goal);
    // Carrying only what it no longer wants is short, not ready.
    let v2 = View { have: [0, 4], ..v };
    assert_eq!(t.status(3, &v2), Status::Short { short: [5, 0] });
    // A partial contribution does not fill it.
    let v3 = View { have: [2, 0], ..v };
    assert!(!t.plan(3, &v3).expect("ready").fills_goal);
    // Filled, it is done for every player.
    v.server.set(3);
    assert_eq!(t.status(3, &v), Status::Done);
}

#[test]
fn a_node_can_need_a_server_goal_and_a_score() {
    let t = tree();
    let mut v = view([9, 9], &[]);
    v.mine.set(1);
    assert_eq!(
        t.status(4, &v),
        Status::Locked {
            needs: vec![Need::Node("generator"), Need::Score(60)]
        }
    );
    v.server.set(3);
    assert_eq!(
        t.status(4, &v),
        Status::Locked {
            needs: vec![Need::Score(60)]
        }
    );
    v.score = 60;
    assert_eq!(t.status(4, &v), Status::Ready);
    // A player's own unlocks never stand in for the server's.
    let mine_only = View {
        server: Unlocks::NONE,
        mine: v.mine.with(3),
        ..v
    };
    assert!(!t.needs_met(4, &mine_only));
}

#[test]
fn the_way_to_a_target_starts_at_its_first_reachable_need() {
    let t = tree();
    let engine = t.index("engine").expect("engine");
    // The engine wants a score no building can give: blocked.
    let v = view([9, 9], &[]);
    assert_eq!(
        t.next_toward(engine, &v),
        Next::Blocked {
            needs: vec![Need::Score(60)]
        }
    );
    // With the score: the generator first, which needs the toolkit first.
    let v = View { score: 60, ..v };
    assert_eq!(t.next_toward(engine, &v), Next::Build(1), "the toolkit");
    // Carrying nothing, the toolkit is what to save for.
    let empty = View { have: [0, 0], ..v };
    assert_eq!(
        t.next_toward(engine, &empty),
        Next::Save {
            node: 1,
            short: [4, 1]
        }
    );
    // Toolkit done: give to the generator.
    let mut v = v;
    v.mine.set(1);
    assert_eq!(t.next_toward(engine, &v), Next::Build(3));
    // Generator filled: the engine itself.
    v.server.set(3);
    assert_eq!(t.next_toward(engine, &v), Next::Build(engine));
}

#[test]
fn research_done_and_a_goal_filled_are_reached_and_a_recipe_never_is() {
    let t = tree();
    let mut v = view([9, 9], &[]);
    v.mine.set(1);
    assert_eq!(t.next_toward(1, &v), Next::Reached, "research done");
    v.server.set(3);
    assert_eq!(t.next_toward(3, &v), Next::Reached, "goal filled");
    v.mine.set(0);
    assert_eq!(t.next_toward(0, &v), Next::Build(0), "built again");
    assert!(matches!(t.next_toward(99, &v), Next::Blocked { .. }));
}

#[test]
fn unlocks_are_bits() {
    let u = Unlocks::NONE.with(0).with(127);
    assert!(u.has(0) && u.has(127) && !u.has(5) && !u.has(500));
    assert_eq!(u.count(), 2);
    assert_eq!(Unlocks::from_bits(u.bits()), u);
    assert_eq!(u.iter().collect::<Vec<_>>(), [0, 127]);
}
