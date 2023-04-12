use anyhow::Result;
use serde::Deserialize;
use std::cell::RefCell;
use std::fmt::Debug;
use std::fs::{create_dir_all, read_to_string};
use std::path::Path;
use std::str::FromStr;
use tracing::metadata::LevelFilter;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{filter::Targets, fmt, prelude::*, EnvFilter, Layer, Registry};

mod logger;
mod utils;

use logger::{FileLogger, GlobalLogger, LogFormat, StreamLogger};

#[derive(Debug, Deserialize)]
struct LogConfig {
    global: GlobalLogger,
    stream_logger: StreamLogger,
    file_logger: Option<Vec<FileLogger>>,
}

pub struct DynamicLogger {
    config: LogConfig,
    layers: RefCell<Vec<Box<dyn Layer<Registry> + Send + Sync>>>,
    guards: RefCell<Vec<WorkerGuard>>,
}

impl DynamicLogger {
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let config = toml::from_str(&read_to_string(path.as_ref())?)?;
        Ok(Self {
            config,
            layers: RefCell::new(Vec::new()),
            guards: RefCell::new(Vec::new()),
        })
    }

    pub fn with_file_logger(self) -> Result<Self> {
        if self.config.global.options.enabled {
            self.init_filelogger()?;
        }

        Ok(self)
    }

    pub fn with_stdout(self) -> Result<Self> {
        if self.config.global.options.enabled {
            self.init_stdout()?;
        }
        Ok(self)
    }

    pub fn init(&self) {
        let global = &self.config.global;
        if global.options.enabled {
            let stream_targets = Targets::from_str(&self.config.stream_logger.modules.join(","))
                .map(|targets| {
                    targets
                        .into_iter()
                        .map(|(filter, _)| (filter, LevelFilter::OFF))
                        .collect::<Targets>()
                });

            let layer = fmt::layer()
                .with_file(global.options.file)
                .with_line_number(global.options.line_number)
                .with_thread_names(global.options.thread_name)
                .with_thread_ids(global.options.thread_id);

            let env_layer = match global.options.format {
                LogFormat::Full => layer.boxed(),
                LogFormat::Compact => layer.compact().boxed(),
                LogFormat::Pretty => layer.pretty().boxed(),
                LogFormat::Json => layer.json().boxed(),
            };

            let mut ref_layers = self.layers.borrow_mut();
            if let Ok(st) = stream_targets {
                ref_layers.push(
                    env_layer
                        .with_filter(st.with_default(LevelFilter::from_level(global.log_level)))
                        .boxed(),
                );
            } else {
                let envfilter =
                    EnvFilter::from_default_env().add_directive(global.log_level.into());
                ref_layers.push(env_layer.with_filter(envfilter).boxed());
            }
        }

        tracing_subscriber::registry()
            .with(self.layers.take())
            .init();
    }

    fn register_filelogger_target(&self, entry: &FileLogger) -> Result<()> {
        let log_dir = &entry.path;
        create_dir_all(log_dir)?;
        let appender = tracing_appender::rolling::never(log_dir, &entry.filename);
        let (file_writer, guard) = tracing_appender::non_blocking(appender);
        self.guards.borrow_mut().push(guard);

        let options = &entry.options;
        if let Ok(file_targets) = Targets::from_str(&entry.modules.join(",")) {
            let file_layer = fmt::Layer::new()
                .with_writer(file_writer)
                .with_ansi(false)
                .with_file(options.file)
                .with_line_number(options.line_number)
                .with_thread_names(options.thread_name)
                .with_thread_ids(options.thread_id);
            let layer = match options.format {
                LogFormat::Full => file_layer.with_filter(file_targets).boxed(),
                LogFormat::Compact => file_layer.compact().with_filter(file_targets).boxed(),
                LogFormat::Pretty => file_layer.pretty().with_filter(file_targets).boxed(),
                LogFormat::Json => file_layer.json().with_filter(file_targets).boxed(),
            };

            self.layers.borrow_mut().push(layer);
            Ok(())
        } else {
            anyhow::bail!(
                "Error parsing file targets. failed to initialize file logging for {entry:#?}"
            )
        }
    }

    #[must_use]
    pub fn add_layer(self, layer: Box<dyn Layer<Registry> + Send + Sync>) -> Self {
        self.layers.borrow_mut().push(layer);
        self
    }

    #[must_use]
    pub fn add_layer_with_stream_logger_targets(
        self,
        layer: Box<dyn Layer<Registry> + Send + Sync>,
    ) -> Self {
        let kekw = if let Ok(file_targets) =
            Targets::from_str(&self.config.stream_logger.modules.join(","))
        {
            layer.with_filter(file_targets).boxed()
        } else {
            layer
        };

        self.layers.borrow_mut().push(kekw);
        self
    }

    #[must_use]
    pub fn add_layers<T>(self, layers: T) -> Self
    where
        T: IntoIterator<Item = Box<dyn Layer<Registry> + Send + Sync>>,
    {
        {
            let mut ref_layers = self.layers.borrow_mut();
            for layer in layers {
                ref_layers.push(layer);
            }
        }
        self
    }
}

pub trait DynamicLogging {
    fn init_stdout(&self) -> Result<()>;
    fn init_filelogger(&self) -> Result<()>;
}

impl DynamicLogging for DynamicLogger {
    fn init_stdout(&self) -> Result<()> {
        let stream_logger = &self.config.stream_logger;
        if !stream_logger.options.enabled {
            return Ok(());
        }

        match Targets::from_str(&stream_logger.modules.join(",")) {
            Ok(targets) => {
                let stream_layer = fmt::Layer::new()
                    .with_writer(std::io::stdout)
                    .with_file(stream_logger.options.file)
                    .with_line_number(stream_logger.options.line_number)
                    .with_thread_names(stream_logger.options.thread_name)
                    .with_thread_ids(stream_logger.options.thread_id);
                let layer = match stream_logger.options.format {
                    LogFormat::Full => stream_layer
                        .with_ansi(stream_logger.color)
                        .with_filter(targets)
                        .boxed(),
                    LogFormat::Compact => stream_layer
                        .with_ansi(stream_logger.color)
                        .compact()
                        .with_filter(targets)
                        .boxed(),
                    LogFormat::Pretty => stream_layer.pretty().with_filter(targets).boxed(),
                    LogFormat::Json => stream_layer.json().with_filter(targets).boxed(),
                };

                self.layers.borrow_mut().push(layer);
                Ok(())
            }
            Err(msg) => {
                anyhow::bail!(
                    "Error parsing file targets. stdout logging failed to initialize, config has errors: {:#?}. Context: {}",
                    stream_logger,
                    msg
                )
            }
        }
    }

    fn init_filelogger(&self) -> Result<()> {
        if let Some(file_logger_table) = &self.config.file_logger {
            for entry in file_logger_table.iter().filter(|file| file.options.enabled) {
                self.register_filelogger_target(entry)?;
            }
            Ok(())
        } else {
            anyhow::bail!("Error parsing file logger table, there were no entries found.")
        }
    }
}
