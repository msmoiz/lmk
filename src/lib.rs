use std::{fs, panic::PanicInfo, path::PathBuf, time::SystemTime};

use chrono::{DateTime, Utc};
use indoc::indoc;
use serde::Serialize;

/// A crash report.
#[derive(Serialize)]
struct Report {
    captured_at: String,
    package_name: String,
    package_version: String,
    binary_name: Option<String>,
    working_dir: Option<PathBuf>,
    operating_system: String,
    panic_message: Option<String>,
    panic_location: String,
    backtrace: String,
}

impl Report {
    /// Creates a new crash report.
    pub fn new(metadata: &Metadata, panic: &PanicInfo) -> Self {
        let captured_at = DateTime::<Utc>::from(SystemTime::now()).to_rfc3339();
        let binary_name = std::env::args().next();
        let working_dir = std::env::current_dir().ok();
        let os = std::env::consts::OS.to_owned();

        let panic_message = panic
            .payload()
            .downcast_ref::<&str>()
            .map(|message| message.to_string());

        let panic_location = panic
            .location()
            .map(|loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column()))
            .expect("panic location should always be set");

        let backtrace = std::backtrace::Backtrace::force_capture().to_string();

        Self {
            captured_at,
            package_name: metadata.name.clone(),
            package_version: metadata.version.clone(),
            binary_name,
            working_dir,
            operating_system: os,
            panic_message,
            panic_location,
            backtrace,
        }
    }
}

/// Information about the host application that is used to populate the crash
/// report and the error message shown to users.
#[derive(Clone)]
pub struct Metadata {
    /// The name of the host application.
    name: String,
    /// The version of the host application.
    version: String,
    /// The URL of the GitHub repository for the host application.
    repository: String,
}

impl Metadata {
    /// Create a new metadata object.
    pub fn new(name: String, version: String, repository: String) -> Self {
        Self {
            name,
            version,
            repository,
        }
    }
}

/// Initializes a crash reporter.
///
/// This installs a panic hook that will (on panic) write a crash report to file
/// and inform the user of the crash. The crash report is written to a TOML file
/// in the OS-specific temp directory with a unique id. If the report cannot be
/// written to file, it is printed to stderr instead as a last-ditch effort. The
/// message displayed to users directs them to the crash report and encourages
/// them to raise an issue on GitHub in the relevant repository.
///
/// The panic hook is only registered when the following conditions are met:
///
/// * The executable is a release build.
/// * The RUST_BACKTRACE environment variable is not set.
///
/// This method uses information about the host application (name, version,
/// repository) to populate the crash report and error message. You can
/// construct a [`Metadata`] object manually and pass it to the method. If no
/// [`Metadata`] is provided, it is inferred from the information in the host
/// application's package manifest.
#[macro_export]
macro_rules! init_crash_reporter {
    ($metadata: expr) => {
        $crate::init_crash_reporter($metadata)
    };

    () => {
        $crate::init_crash_reporter($crate::Metadata::new(
            env!("CARGO_PKG_NAME").to_owned(),
            env!("CARGO_PKG_VERSION").to_owned(),
            env!("CARGO_PKG_REPOSITORY").to_owned(),
        ));
    };
}

#[doc(hidden)]
pub fn init_crash_reporter(metadata: Metadata) {
    let is_release_mode = cfg!(not(debug_assertions));
    let backtrace_enabled = std::env::var("RUST_BACKTRACE").is_ok();

    if is_release_mode && !backtrace_enabled {
        std::panic::set_hook(Box::new(move |panic| {
            let report = Report::new(&metadata, panic);
            let content = toml::to_string_pretty(&report).expect("report should serialize to toml");
            let output_dir = std::env::temp_dir()
                .join(&report.package_name)
                .join("crash");

            let report_id = ulid::Generator::new()
                .generate()
                .expect("ulid gen should not error")
                .to_string();

            let report_path = output_dir.join(report_id).with_extension("toml");

            let result =
                fs::create_dir_all(output_dir).and_then(|_| fs::write(&report_path, &content));

            if let Err(e) = result {
                eprintln!(
                    "error: failed to save crash report to {}",
                    report_path.display()
                );
                eprintln!("{sep}\n{e}\n{sep}", sep = "-".repeat(20));
                eprintln!("error: writing crash report directly to stderr");
                eprintln!("{sep}\n{content}\n{sep}", sep = "-".repeat(20));
                return;
            }

            eprintln!(
                indoc! {
                "{} has crashed!

                A crash report has been saved to {}. To get support for this problem,
                please raise an issue on GitHub at {}/issues and include the crash
                report to help us better diagnose the problem."},
                report.package_name,
                report_path.display(),
                metadata.repository
            );
        }))
    }
}
