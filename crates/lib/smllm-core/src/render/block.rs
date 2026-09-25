//! The `<smllm>` fence every piece of agent text is built from (TURN-12).
// @zen-component: TURN-Render

use core::fmt::Write as _;

use crate::engine::{Offer, ParamView};
use crate::prelude::*;

/// Builds one `<smllm>…</smllm>` block.
pub(crate) struct Block {
    out: String,
}

impl Block {
    /// Open a block with its header line.
    // @zen-impl: TURN-12_AC-1
    pub(crate) fn open(header: &str) -> Self {
        let mut out = String::from("<smllm>\n");
        out.push_str(header);
        out.push('\n');
        Self { out }
    }

    /// A plain line.
    pub(crate) fn line(&mut self, line: &str) -> &mut Self {
        self.out.push_str(line);
        self.out.push('\n');
        self
    }

    /// Several lines.
    pub(crate) fn lines(&mut self, lines: &[String]) -> &mut Self {
        for l in lines {
            self.line(l);
        }
        self
    }

    /// Author text, fenced in `<instructions>`; omitted when empty.
    pub(crate) fn instructions(&mut self, prompts: &[String]) -> &mut Self {
        if prompts.is_empty() {
            return self;
        }
        self.out.push_str("<instructions>\n");
        for (i, p) in prompts.iter().enumerate() {
            if i > 0 {
                self.out.push('\n');
            }
            self.out.push_str(p.trim_end());
            self.out.push('\n');
        }
        self.out.push_str("</instructions>\n");
        self
    }

    /// The menu: the call line, then `<events>` (TURN-2).
    // @zen-impl: TURN-2_AC-1
    pub(crate) fn events(&mut self, key: &str, offers: &[Offer]) -> &mut Self {
        let _ = writeln!(
            self.out,
            "Fire one event: smllm({{ session: \"{key}\", event, params }})"
        );
        self.out.push_str("<events>\n");
        for offer in offers {
            self.out.push_str("- ");
            self.out.push_str(&offer.name);
            if let Some(d) = &offer.description {
                self.out.push_str(" — ");
                self.out.push_str(d);
            }
            self.out.push('\n');
            for p in &offer.params {
                self.param(p);
            }
        }
        self.out.push_str("</events>\n");
        self
    }

    fn param(&mut self, p: &ParamView) {
        let _ = write!(
            self.out,
            "    {} ({}",
            p.name,
            if p.required { "required" } else { "optional" }
        );
        if !p.enum_values.is_empty() {
            let _ = write!(self.out, ", one of: {}", p.enum_values.join(", "));
        }
        if let Some(pat) = &p.pattern {
            let _ = write!(self.out, ", pattern: {pat}");
        }
        self.out.push(')');
        if let Some(d) = &p.description {
            self.out.push_str(": ");
            self.out.push_str(d);
        }
        self.out.push('\n');
    }

    /// Close the fence and return the text.
    pub(crate) fn close(&mut self) -> String {
        let mut out = core::mem::take(&mut self.out);
        out.push_str("</smllm>\n");
        out
    }
}
