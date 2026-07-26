use crate::domain::sexpr::formatter::Formatter;
use crate::domain::sexpr::tree::{NodeKind, SyntaxTree};
use crate::domain::sexpr::types::NodeId;

impl Formatter {
    pub(in crate::domain::sexpr::formatter) fn format_loop_form(
        &self,
        tree: &SyntaxTree,
        node_id: NodeId,
        depth: usize,
        head: &str,
        output: &mut String,
    ) {
        let node = tree.node(node_id);
        let delimiter = self.list_delimiter(node);
        let continuation_column = self.continuation_column(depth, head.len().saturating_add(2));
        output.push(delimiter.open());
        self.format_node(tree, node.children[0], depth + 1, output);

        let mut position = 1;
        let mut conditional_clause_open = false;
        while position < node.children.len() {
            let clause_head = self.atom_text(tree, node.children[position]);
            let nested_action = conditional_clause_open
                && clause_head.is_some_and(Self::is_loop_conditional_action_keyword);

            if position == 1 {
                output.push(' ');
            } else {
                output.push('\n');
                output.push_str(&" ".repeat(continuation_column));
                if nested_action {
                    output.push_str(&self.indent(1));
                }
            }

            let clause_start = position;
            position += 1;
            while position < node.children.len()
                && !self.is_loop_clause_start(tree, node.children[position])
            {
                position += 1;
            }

            self.format_loop_clause(
                tree,
                &node.children[clause_start..position],
                depth + 1,
                output,
            );

            match clause_head.map(str::to_ascii_lowercase).as_deref() {
                Some("if" | "when" | "unless") => conditional_clause_open = true,
                Some("else") => conditional_clause_open = true,
                Some("end") => conditional_clause_open = false,
                Some(keyword) if Self::is_loop_conditional_action_keyword(keyword) => {
                    conditional_clause_open = false;
                }
                _ => {}
            }
        }

        output.push(delimiter.close());
    }

    pub(in crate::domain::sexpr::formatter) fn format_loop_clause(
        &self,
        tree: &SyntaxTree,
        children: &[NodeId],
        depth: usize,
        output: &mut String,
    ) {
        for (position, child) in children.iter().enumerate() {
            if position > 0 {
                output.push(' ');
            }
            self.format_inline_or_node(tree, *child, depth, output);
        }
    }

    pub(in crate::domain::sexpr::formatter) fn is_loop_clause_start(
        &self,
        tree: &SyntaxTree,
        node_id: NodeId,
    ) -> bool {
        self.atom_text(tree, node_id)
            .is_some_and(Self::is_loop_clause_keyword)
    }

    pub(in crate::domain::sexpr::formatter) fn atom_text<'a>(
        &self,
        tree: &'a SyntaxTree,
        node_id: NodeId,
    ) -> Option<&'a str> {
        let node = tree.node(node_id);
        (node.kind == NodeKind::Atom).then(|| node.span.slice(&tree.source))
    }

    pub(in crate::domain::sexpr::formatter) fn is_loop_clause_keyword(keyword: &str) -> bool {
        let keyword = keyword.to_ascii_lowercase();
        matches!(
            keyword.as_str(),
            "for"
                | "as"
                | "with"
                | "and"
                | "repeat"
                | "initially"
                | "finally"
                | "while"
                | "until"
                | "always"
                | "never"
                | "thereis"
                | "if"
                | "when"
                | "unless"
                | "else"
                | "end"
        ) || Self::is_loop_conditional_action_keyword(&keyword)
    }

    pub(in crate::domain::sexpr::formatter) fn is_loop_conditional_action_keyword(
        keyword: &str,
    ) -> bool {
        matches!(
            keyword.to_ascii_lowercase().as_str(),
            "do" | "doing"
                | "return"
                | "collect"
                | "collecting"
                | "append"
                | "appending"
                | "nconc"
                | "nconcing"
                | "count"
                | "counting"
                | "sum"
                | "summing"
                | "maximize"
                | "maximizing"
                | "minimize"
                | "minimizing"
        )
    }
}
