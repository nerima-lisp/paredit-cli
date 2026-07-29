//! Clone genealogy: which copy came first, and how the rest followed.
//!
//! A clone class says five places share a shape. It does not say which of them
//! is the original, and that is the fact that decides what to do: the oldest
//! member is usually the one to keep and turn into the helper, the newest tells
//! you the copying is still happening, and the gap between them tells you
//! whether this is one bad afternoon in 2019 or an ongoing habit.
//!
//! History comes from `git blame` over each member's line range, through a port
//! so the ordering logic stays testable without a repository. Attribution is
//! per member by its *oldest* line: a form is at least as old as its oldest
//! surviving line, and asking for the newest — which is what `inspect blame`
//! wants, since it is answering "who should I ask about this" — would date
//! every member to whenever it was last reformatted.
//!
//! Degrades the way `blame` does. No repository, no `git`, an untracked file:
//! the member is reported with its history unavailable and a role of `unknown`,
//! not with a fabricated date.

use std::path::Path as FsPath;
use std::sync::Arc;

use crate::form_similarity::CloneType;
use crate::similarity_report::domain::SimilarityFormReport;

use super::class::{CloneClass, CloneClassReport, ExtractionEstimate};

/// Seconds in a day, for turning commit timestamps into a lag someone can read.
const SECONDS_PER_DAY: i64 = 86_400;

/// What `git` knew about one clone member.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MemberHistory {
    pub commit: Option<String>,
    pub author: Option<String>,
    /// ISO-8601, as the port formatted it.
    pub date: Option<String>,
    /// Author time in seconds since the epoch, which is what the ordering uses.
    pub timestamp: Option<i64>,
    /// Why there is no history, when there is none.
    pub unavailable: Option<String>,
}

impl MemberHistory {
    #[must_use]
    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            unavailable: Some(reason.into()),
            ..Self::default()
        }
    }

    #[must_use]
    pub const fn is_available(&self) -> bool {
        self.timestamp.is_some()
    }
}

/// Where `git blame` gets asked. Implemented over a real repository by the CLI
/// and over a fixture by the tests.
pub trait CloneHistoryPort {
    /// The oldest commit touching `lines` (1-based, inclusive) of `path`.
    fn oldest_commit(&self, path: &FsPath, lines: std::ops::RangeInclusive<usize>)
    -> MemberHistory;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloneOrigin {
    /// The oldest member of the class.
    Origin,
    /// Everything dated after the origin.
    Copy,
    /// No history, so no claim.
    Unknown,
}

impl CloneOrigin {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Origin => "origin",
            Self::Copy => "copy",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CloneGenealogyMember {
    pub form: Arc<SimilarityFormReport>,
    pub start_line: usize,
    pub end_line: usize,
    pub history: MemberHistory,
    pub role: CloneOrigin,
    /// Days between the origin's commit and this member's. `None` for the
    /// origin itself and for members without history.
    pub lag_days: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CloneGenealogy {
    pub rank: usize,
    pub clone_type: CloneType,
    pub members: Vec<CloneGenealogyMember>,
    pub origin_commit: Option<String>,
    pub origin_author: Option<String>,
    pub origin_date: Option<String>,
    /// Days between the oldest and newest member. `None` unless at least two
    /// members have history.
    pub span_days: Option<i64>,
    pub dated_members: usize,
    pub undated_members: usize,
    pub file_count: usize,
    pub extraction: ExtractionEstimate,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CloneGenealogyReport {
    pub genealogies: Vec<CloneGenealogy>,
    pub total_classes: usize,
    pub dated_members: usize,
    pub undated_members: usize,
    /// Distinct reasons the port gave for the members it could not date,
    /// deduplicated so a repository-wide failure reports once.
    pub unavailable_reasons: Vec<String>,
}

pub fn build_clone_genealogy_report(
    classes: &CloneClassReport,
    port: &impl CloneHistoryPort,
) -> CloneGenealogyReport {
    let mut genealogies = Vec::with_capacity(classes.classes.len());
    let mut dated_members = 0;
    let mut undated_members = 0;
    let mut unavailable_reasons = Vec::new();

    for class in &classes.classes {
        let genealogy = build_genealogy(class, port);
        dated_members += genealogy.dated_members;
        undated_members += genealogy.undated_members;
        for member in &genealogy.members {
            if let Some(reason) = &member.history.unavailable {
                if !unavailable_reasons.iter().any(|seen| seen == reason) {
                    unavailable_reasons.push(reason.clone());
                }
            }
        }
        genealogies.push(genealogy);
    }

    // Ranks follow the class ranks, which are already ordered by what
    // extracting the class would save. Re-sorting by age would bury the
    // expensive classes behind old trivial ones.
    for (index, genealogy) in genealogies.iter_mut().enumerate() {
        genealogy.rank = index + 1;
    }

    CloneGenealogyReport {
        total_classes: classes.total_classes,
        genealogies,
        dated_members,
        undated_members,
        unavailable_reasons,
    }
}

fn build_genealogy(class: &CloneClass, port: &impl CloneHistoryPort) -> CloneGenealogy {
    let mut members = class
        .members
        .iter()
        .map(|form| {
            let (start_line, end_line) = line_range(form);
            let history = port.oldest_commit(form.path(), start_line..=end_line);
            CloneGenealogyMember {
                form: Arc::clone(form),
                start_line,
                end_line,
                history,
                role: CloneOrigin::Unknown,
                lag_days: None,
            }
        })
        .collect::<Vec<_>>();

    // Oldest first, then by location so undated members keep a stable order
    // instead of drifting with the hash iteration that produced the class.
    members.sort_by(|left, right| {
        left.history
            .timestamp
            .is_none()
            .cmp(&right.history.timestamp.is_none())
            .then_with(|| left.history.timestamp.cmp(&right.history.timestamp))
            .then_with(|| left.form.path().cmp(right.form.path()))
            .then_with(|| {
                left.form
                    .span()
                    .start()
                    .get()
                    .cmp(&right.form.span().start().get())
            })
    });

    let origin_timestamp = members
        .iter()
        .filter_map(|member| member.history.timestamp)
        .min();
    let newest_timestamp = members
        .iter()
        .filter_map(|member| member.history.timestamp)
        .max();

    let mut origin_assigned = false;
    for member in &mut members {
        let Some(timestamp) = member.history.timestamp else {
            member.role = CloneOrigin::Unknown;
            continue;
        };
        // Ties go to the first in the sorted order, so a class whose members
        // all landed in one commit reports exactly one origin.
        if !origin_assigned && Some(timestamp) == origin_timestamp {
            member.role = CloneOrigin::Origin;
            origin_assigned = true;
            continue;
        }
        member.role = CloneOrigin::Copy;
        member.lag_days = origin_timestamp.map(|origin| (timestamp - origin) / SECONDS_PER_DAY);
    }

    let origin = members
        .iter()
        .find(|member| member.role == CloneOrigin::Origin);
    let dated_members = members
        .iter()
        .filter(|member| member.history.is_available())
        .count();
    let mut files = members
        .iter()
        .map(|member| member.form.path().to_path_buf())
        .collect::<Vec<_>>();
    files.sort_unstable();
    files.dedup();

    CloneGenealogy {
        rank: 0,
        clone_type: class.clone_type,
        origin_commit: origin.and_then(|member| member.history.commit.clone()),
        origin_author: origin.and_then(|member| member.history.author.clone()),
        origin_date: origin.and_then(|member| member.history.date.clone()),
        span_days: match (origin_timestamp, newest_timestamp) {
            (Some(oldest), Some(newest)) if dated_members >= 2 => {
                Some((newest - oldest) / SECONDS_PER_DAY)
            }
            _ => None,
        },
        undated_members: members.len() - dated_members,
        dated_members,
        file_count: files.len(),
        extraction: class.extraction,
        members,
    }
}

/// The 1-based line range a form occupies in its own file.
///
/// Counted from the file the form was sliced out of, which the report still
/// holds, so this needs no second read of anything.
fn line_range(form: &SimilarityFormReport) -> (usize, usize) {
    let source = form.text().source();
    let span = form.span();
    let start = line_of(source, span.start().get());
    let end = line_of(source, span.end().get().saturating_sub(1)).max(start);
    (start, end)
}

fn line_of(source: &str, offset: usize) -> usize {
    source
        .bytes()
        .take(offset.min(source.len()))
        .filter(|&byte| byte == b'\n')
        .count()
        + 1
}
