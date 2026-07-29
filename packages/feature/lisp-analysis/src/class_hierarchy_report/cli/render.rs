use paredit_core_cli::CliResult;

use paredit_core_cli::args::ReportFormat;

use crate::class_hierarchy_report::usecase::ClassFinding;
use paredit_core_cli::report::graph::{EdgeStyle, Graph, NodeShape};
use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Draws the class hierarchy: an edge from each class to each direct
/// superclass, in the order written.
///
/// Superclass order is the class precedence list, so the label carries it.
/// Without it the picture loses the one thing about multiple inheritance a
/// reader most needs — which parent wins a slot conflict — and would leave two
/// indistinguishable arrows where the source has an ordered pair.
///
/// A class that shadows a superclass slot is drawn as a container rather than a
/// plain box, so the report's actionable finding survives into the picture; a
/// superclass no scanned file defines is drawn open, because its slots could
/// not be attributed.
pub fn class_hierarchy_drawing(reports: &[FileFindings<ClassFinding>]) -> Graph {
    let mut graph = Graph::new("inspect class-hierarchy");

    for report in reports {
        let group = report.path.display().to_string();
        for class in &report.findings {
            graph.add_node(
                class.name.clone(),
                if class.shadowed_slots.is_empty() {
                    NodeShape::Definition
                } else {
                    NodeShape::Container
                },
                Some(group.clone()),
            );
        }
    }

    for report in reports {
        for class in &report.findings {
            for (index, superclass) in class.superclasses.iter().enumerate() {
                let unresolved = class.unresolved_superclasses.contains(superclass);
                if unresolved {
                    graph.add_node(superclass.clone(), NodeShape::External, None);
                }
                graph.add_edge(
                    class.name.clone(),
                    superclass.clone(),
                    (class.superclasses.len() > 1).then(|| format!("{}", index + 1)),
                    if unresolved {
                        EdgeStyle::External
                    } else {
                        EdgeStyle::Internal
                    },
                );
            }
        }
    }

    graph
}

pub fn print_shadowed_slot_report(
    reports: &[FileFindings<ClassFinding>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> CliResult<()> {
    print_report("inspect class-hierarchy", reports, policy, output)
}
