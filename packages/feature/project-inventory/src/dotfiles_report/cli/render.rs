use std::io::Write;

use paredit_core_cli::args::OutputFormat;
use paredit_core_cli::error::{CliError, CliResult};
use paredit_core_cli::shared::terminal_safe;

use crate::dotfiles_report::domain::Report;

pub fn print_report(report: &Report, output: OutputFormat) -> CliResult<()> {
    let stdout = std::io::stdout();
    render(report, output, &mut stdout.lock())
}

fn render(report: &Report, output: OutputFormat, writer: &mut impl Write) -> CliResult<()> {
    let result = match output {
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&report.to_json())?;
            writeln!(writer, "{json}")
        }
        OutputFormat::Text => render_text(report, writer),
    };
    result.map_err(CliError::io("writing dotfiles inventory"))
}

fn render_text(report: &Report, writer: &mut impl Write) -> std::io::Result<()> {
    writeln!(
        writer,
        "present={} complete={}",
        report.present, report.complete
    )?;
    for entry in &report.entries {
        writeln!(
            writer,
            "{}\t{}\t{}\t{}\t{}",
            terminal_safe(&entry.path),
            entry.kind.as_str(),
            entry.category.as_str(),
            entry
                .bytes
                .map_or_else(|| "-".to_owned(), |bytes| bytes.to_string()),
            entry.status.as_str(),
        )?;
    }
    for diagnostic in &report.diagnostics {
        writeln!(
            writer,
            "{}\t{}\t{}",
            terminal_safe(&diagnostic.path),
            diagnostic.code,
            terminal_safe(&diagnostic.message),
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dotfiles_report::domain::{Category, Entry, EntryKind, EntryStatus};

    #[test]
    fn text_escapes_filename_record_controls() {
        let report = Report {
            present: true,
            complete: true,
            entries: vec![Entry {
                path: ".paredit/a\n\u{1b}[31m".to_owned(),
                kind: EntryKind::File,
                category: Category::Other,
                bytes: Some(0),
                status: EntryStatus::Inventoried,
            }],
            diagnostics: vec![],
        };
        let mut bytes = Vec::new();
        render(&report, OutputFormat::Text, &mut bytes).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert_eq!(text.lines().count(), 2);
        assert!(!text.contains('\u{1b}'));
    }

    #[test]
    fn failed_output_is_not_success() {
        struct Broken;
        impl Write for Broken {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::Error::from(std::io::ErrorKind::BrokenPipe))
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let report = Report {
            present: false,
            complete: true,
            entries: vec![],
            diagnostics: vec![],
        };
        assert!(render(&report, OutputFormat::Json, &mut Broken).is_err());
    }
}
