use crate::Symbol;

/// Configuration for the turtle interpreter.
#[derive(Debug, Clone)]
pub struct TurtleConfig {
    /// Starting X position.
    pub start_x: f32,
    /// Starting Y position.
    pub start_y: f32,
    /// Starting heading in radians (0 = right/east, PI/2 = down/south).
    pub initial_heading: f32,
    /// Distance to move per F/f step.
    pub step_length: f32,
    /// Angle change per +/- turn (radians).
    pub angle_delta: f32,
    /// Multiplier applied to step_length when '>' is encountered.
    pub step_scale: f32,
    /// Clamp bounds — turtle stops producing segments outside these.
    pub bounds: Option<(f32, f32, f32, f32)>, // (min_x, min_y, max_x, max_y)
}

impl Default for TurtleConfig {
    fn default() -> Self {
        Self {
            start_x: 0.0,
            start_y: 0.0,
            initial_heading: -std::f32::consts::FRAC_PI_2, // north
            step_length: 10.0,
            angle_delta: std::f32::consts::FRAC_PI_4, // 45 degrees
            step_scale: 0.8,
            bounds: None,
        }
    }
}

/// A line segment produced by the turtle.
#[derive(Debug, Clone)]
pub struct Segment {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
    /// Depth of the branch stack when this segment was drawn.
    /// 0 = main trunk, 1+ = branches. Useful for varying road width.
    pub depth: u16,
    /// The symbol character that produced this segment.
    pub symbol: char,
}

/// Turtle state for interpretation.
struct Turtle {
    x: f32,
    y: f32,
    heading: f32,
    step: f32,
    depth: u16,
}

/// Interpret an L-system string using turtle graphics.
///
/// Standard symbols:
/// - `F` — move forward, drawing a segment
/// - `f` — move forward without drawing
/// - `+` — turn left by angle_delta
/// - `-` — turn right by angle_delta
/// - `[` — push state (start branch)
/// - `]` — pop state (end branch)
/// - `>` — multiply step length by step_scale
/// - `<` — divide step length by step_scale
///
/// Other characters are ignored (treated as no-ops).
/// Parametric symbols: if a symbol has params, the first param overrides step_length
/// for that step.
pub fn interpret(symbols: &[Symbol], config: &TurtleConfig) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut stack: Vec<Turtle> = Vec::new();

    let mut turtle = Turtle {
        x: config.start_x,
        y: config.start_y,
        heading: config.initial_heading,
        step: config.step_length,
        depth: 0,
    };

    for sym in symbols {
        match sym.ch {
            'F' | 'G' => {
                // Move forward, drawing a segment
                let step = if sym.params.is_empty() { turtle.step } else { sym.params[0] };
                let nx = turtle.x + step * turtle.heading.cos();
                let ny = turtle.y + step * turtle.heading.sin();

                let in_bounds = config.bounds.is_none_or(|(min_x, min_y, max_x, max_y)| {
                    nx >= min_x && nx <= max_x && ny >= min_y && ny <= max_y
                });

                if in_bounds {
                    segments.push(Segment {
                        x1: turtle.x,
                        y1: turtle.y,
                        x2: nx,
                        y2: ny,
                        depth: turtle.depth,
                        symbol: sym.ch,
                    });
                }

                turtle.x = nx;
                turtle.y = ny;
            }

            'f' => {
                // Move forward without drawing
                let step = if sym.params.is_empty() { turtle.step } else { sym.params[0] };
                turtle.x += step * turtle.heading.cos();
                turtle.y += step * turtle.heading.sin();
            }

            '+' => {
                // Turn left
                let angle = if sym.params.is_empty() { config.angle_delta } else { sym.params[0] };
                turtle.heading -= angle;
            }

            '-' => {
                // Turn right
                let angle = if sym.params.is_empty() { config.angle_delta } else { sym.params[0] };
                turtle.heading += angle;
            }

            '[' => {
                // Push state
                stack.push(Turtle {
                    x: turtle.x,
                    y: turtle.y,
                    heading: turtle.heading,
                    step: turtle.step,
                    depth: turtle.depth,
                });
                turtle.depth += 1;
            }

            ']' => {
                // Pop state
                if let Some(saved) = stack.pop() {
                    turtle = saved;
                }
            }

            '>' => {
                // Scale step down
                turtle.step *= config.step_scale;
            }

            '<'
                // Scale step up
                if config.step_scale > 0.0 => {
                    turtle.step /= config.step_scale;
                }

            _ => {
                // Ignore unknown symbols (they act as placeholders in rules)
            }
        }
    }

    segments
}
