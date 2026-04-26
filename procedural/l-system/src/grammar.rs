use rand::prelude::*;

/// A symbol in the L-system string — a character with optional parameters.
#[derive(Debug, Clone)]
pub struct Symbol {
    pub ch: char,
    pub params: Vec<f32>,
}

impl Symbol {
    pub fn new(ch: char) -> Self {
        Self {
            ch,
            params: Vec::new(),
        }
    }

    pub fn with_params(ch: char, params: Vec<f32>) -> Self {
        Self { ch, params }
    }
}

/// A production rule: symbol → one or more weighted successors.
#[derive(Debug, Clone)]
struct Rule {
    predecessor: char,
    successors: Vec<(f32, Vec<Symbol>)>, // (weight, replacement)
}

impl Rule {
    /// Choose a successor, weighted by probabilities.
    fn choose(&self, rng: &mut impl Rng) -> &[Symbol] {
        if self.successors.len() == 1 {
            return &self.successors[0].1;
        }

        let total: f32 = self.successors.iter().map(|(w, _)| w).sum();
        let mut roll = rng.random::<f32>() * total;
        for (w, syms) in &self.successors {
            roll -= w;
            if roll <= 0.0 {
                return syms;
            }
        }
        &self.successors.last().unwrap().1
    }
}

/// L-system grammar: axiom + production rules.
///
/// Supports deterministic and stochastic rules. Symbols without rules
/// are passed through unchanged (terminals).
pub struct LSystem {
    axiom: Vec<Symbol>,
    rules: Vec<Rule>,
}

impl LSystem {
    /// Create a new L-system with the given axiom string.
    ///
    /// Each character in the axiom becomes a parameterless symbol.
    pub fn new(axiom: &str) -> Self {
        Self {
            axiom: axiom.chars().map(Symbol::new).collect(),
            rules: Vec::new(),
        }
    }

    /// Create with a pre-built axiom symbol list (for parametric systems).
    pub fn from_symbols(axiom: Vec<Symbol>) -> Self {
        Self {
            axiom,
            rules: Vec::new(),
        }
    }

    /// Add a deterministic production rule: predecessor → successor string.
    ///
    /// Each character in the successor becomes a parameterless symbol.
    pub fn add_rule(&mut self, predecessor: char, successor: &str) {
        let syms: Vec<Symbol> = successor.chars().map(Symbol::new).collect();

        if let Some(rule) = self.rules.iter_mut().find(|r| r.predecessor == predecessor) {
            // Replace existing deterministic rule
            rule.successors = vec![(1.0, syms)];
        } else {
            self.rules.push(Rule {
                predecessor,
                successors: vec![(1.0, syms)],
            });
        }
    }

    /// Add a stochastic production rule: predecessor → successor with weight.
    ///
    /// Multiple calls with the same predecessor create weighted alternatives.
    pub fn add_stochastic_rule(&mut self, predecessor: char, weight: f32, successor: &str) {
        let syms: Vec<Symbol> = successor.chars().map(Symbol::new).collect();

        if let Some(rule) = self.rules.iter_mut().find(|r| r.predecessor == predecessor) {
            rule.successors.push((weight, syms));
        } else {
            self.rules.push(Rule {
                predecessor,
                successors: vec![(weight, syms)],
            });
        }
    }

    /// Add a rule with parametric symbols as the successor.
    pub fn add_parametric_rule(&mut self, predecessor: char, successor: Vec<Symbol>) {
        if let Some(rule) = self.rules.iter_mut().find(|r| r.predecessor == predecessor) {
            rule.successors = vec![(1.0, successor)];
        } else {
            self.rules.push(Rule {
                predecessor,
                successors: vec![(1.0, successor)],
            });
        }
    }

    /// Apply production rules `n` times deterministically (no randomness).
    pub fn iterate(&self, n: usize) -> Vec<Symbol> {
        let mut current = self.axiom.clone();
        let mut rng = NoRng;

        for _ in 0..n {
            current = self.step(&current, &mut rng);
        }

        current
    }

    /// Apply production rules `n` times with stochastic choices.
    pub fn iterate_stochastic(&self, n: usize, rng: &mut impl Rng) -> Vec<Symbol> {
        let mut current = self.axiom.clone();

        for _ in 0..n {
            current = self.step(&current, rng);
        }

        current
    }

    /// Single rewriting step.
    fn step(&self, input: &[Symbol], rng: &mut impl Rng) -> Vec<Symbol> {
        let mut output = Vec::with_capacity(input.len() * 2);

        for sym in input {
            if let Some(rule) = self.rules.iter().find(|r| r.predecessor == sym.ch) {
                let replacement = rule.choose(rng);
                // Copy replacement symbols, inheriting parameters if needed
                for r_sym in replacement {
                    let mut new_sym = r_sym.clone();
                    // If replacement symbol has no params but original does, inherit
                    if new_sym.params.is_empty() && !sym.params.is_empty() {
                        new_sym.params = sym.params.clone();
                    }
                    output.push(new_sym);
                }
            } else {
                // Terminal: pass through unchanged
                output.push(sym.clone());
            }
        }

        output
    }
}

/// A dummy RNG that panics if used — for deterministic iteration.
struct NoRng;

impl RngCore for NoRng {
    fn next_u32(&mut self) -> u32 {
        0
    }
    fn next_u64(&mut self) -> u64 {
        0
    }
    fn fill_bytes(&mut self, _dest: &mut [u8]) {}
}
