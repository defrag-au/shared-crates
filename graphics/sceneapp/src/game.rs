//! Shell surface for a dump-folder game. Intents stay in the game crate.

use scenegraph::{Scene, Vec3};

use crate::fx::Fx;

/// Where a pick came from: the eye that looked (the live view's, not
/// rest's) and the point the pointer landed on, both in world units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pointed {
    pub eye: Vec3,
    pub point: Vec3,
}

impl Pointed {
    /// Eye to the point picked: how far away the thing was.
    pub fn reach(&self) -> f32 {
        (self.point - self.eye).length()
    }
}

/// Result of [`Game::tick`] / [`Game::input`]. The shell does not name intents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Applied,
    /// `tick` → game over (stop auto-tick). `input` → no-op (wall).
    Blocked,
}

/// Load from a versioned dump (`tw1`, …). The pane calls `tick` / `input(token)`.
pub trait Game {
    fn dump(&self) -> String;
    fn emit(&self) -> Result<Scene, String>;
    fn tick(&mut self) -> Result<Step, String>;
    fn input(&mut self, token: &str) -> Result<Step, String>;
    /// A pick on a named node (the [`scenegraph::DrawList`] hit). The game
    /// decides whether that node is a move; the shell never does. Default:
    /// no node is a move (`Blocked`).
    fn pick(&mut self, _name: &str) -> Result<Step, String> {
        Ok(Step::Blocked)
    }
    /// A pick with where it came from: the eye that looked and the point on
    /// the node the pointer landed. Every shell calls this; a game that
    /// cares how far away you are ("within reach") reads [`Pointed`].
    /// Default: [`Game::pick`], which ignores the camera.
    fn pick_at(&mut self, name: &str, _at: &Pointed) -> Result<Step, String> {
        self.pick(name)
    }
    /// The view moved (a walk, a turn, an orbit) and the eye is now at
    /// `eye`. A game that reacts to where you stand — a doorway you walk
    /// through, a trigger you step on — returns `Applied` with its cues;
    /// `Blocked` (the default) is "nothing happened".
    fn moved(&mut self, _eye: Vec3) -> Result<Step, String> {
        Ok(Step::Blocked)
    }
    /// A drag that started on node `from` and was released over node `to`
    /// (both [`scenegraph::DrawList`] hits on the scene as it was before the
    /// drag). The game decides whether that is a move; the shell never does.
    /// Default: no drag is a move (`Blocked`).
    fn drag(&mut self, _from: &str, _to: &str) -> Result<Step, String> {
        Ok(Step::Blocked)
    }
    /// Tags whose nodes a pane lets the pointer carry while a drag is held.
    /// Empty (the default) means the game takes no drags: a pane does not
    /// start one, so a press that travels is just a press.
    fn drag_tags(&self) -> &'static [&'static str] {
        &[]
    }
    /// Tags whose nodes are this game's controls (`cell`). The gauntlet
    /// reports a control that cannot be picked anywhere on its face.
    fn control_tags(&self) -> &'static [&'static str] {
        &[]
    }
    /// `(key_name, token)`. Key names are pane strings (`ArrowLeft`, `A`).
    fn bindings(&self) -> &'static [(&'static str, &'static str)];
    /// Cues from the last Applied step. Empty after load or Blocked.
    fn fx(&self) -> &Fx {
        Fx::empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TokenGame;

    impl Game for TokenGame {
        fn dump(&self) -> String {
            "ok".into()
        }
        fn emit(&self) -> Result<Scene, String> {
            Ok(Scene::new())
        }
        fn tick(&mut self) -> Result<Step, String> {
            Ok(Step::Applied)
        }
        fn input(&mut self, token: &str) -> Result<Step, String> {
            match token {
                "left" => Ok(Step::Applied),
                _ => Ok(Step::Blocked),
            }
        }
        fn bindings(&self) -> &'static [(&'static str, &'static str)] {
            &[("ArrowLeft", "left")]
        }
    }

    #[test]
    fn shell_calls_tokens_not_intents() {
        let mut g: Box<dyn Game> = Box::new(TokenGame);
        assert_eq!(g.input("left").unwrap(), Step::Applied);
        assert_eq!(g.input("Left").unwrap(), Step::Blocked);
        assert_eq!(g.bindings()[0].1, "left");
    }

    #[test]
    fn picks_are_not_moves_unless_the_game_says() {
        let mut g: Box<dyn Game> = Box::new(TokenGame);
        assert_eq!(g.pick("c11").unwrap(), Step::Blocked);
    }

    #[test]
    fn drags_are_not_moves_unless_the_game_says() {
        let mut g: Box<dyn Game> = Box::new(TokenGame);
        assert_eq!(g.drag("c11", "c12").unwrap(), Step::Blocked);
        assert!(g.drag_tags().is_empty());
    }
}
