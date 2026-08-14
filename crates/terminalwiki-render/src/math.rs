pub trait MathRenderer {
    fn render_inline(&self, formula: &str) -> String;
    fn render_display(&self, formula: &str) -> Vec<String>;
}

pub struct TerminalTextMathRenderer;

impl MathRenderer for TerminalTextMathRenderer {
    fn render_inline(&self, formula: &str) -> String {
        format!("⟨{}⟩", formula)
    }

    fn render_display(&self, formula: &str) -> Vec<String> {
        formula.lines().map(|l| format!("    {}", l)).collect()
    }
}
