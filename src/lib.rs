use error::DynLogAPIErr;
use serde::Deserialize;
use std::{
    cell::RefCell,
    fmt::Debug,
    fs::{create_dir_all, read_to_string},
    path::Path,
    str::FromStr,
};
use tracing::metadata::LevelFilter;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{EnvFilter, Layer, Registry, filter::Targets, fmt, prelude::*};

pub mod error;
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
    pub fn new(path: impl AsRef<Path>) -> Result<Self, DynLogAPIErr> {
        let config = toml::from_str(&read_to_string(path.as_ref()).map_err(|source| {
            DynLogAPIErr::FileReadError {
                filename: path.as_ref().to_string_lossy().to_string(),
                source,
            }
        })?)?;
        Ok(Self {
            config,
            layers: RefCell::new(Vec::new()),
            guards: RefCell::new(Vec::new()),
        })
    }

    pub fn with_file_logger(self) -> Result<Self, DynLogAPIErr> {
        if self.config.global.options.enabled {
            self.init_filelogger()?;
        }

        Ok(self)
    }

    pub fn with_stdout(self) -> Result<Self, DynLogAPIErr> {
        if self.config.global.options.enabled {
            self.init_stdout()?;
        }
        Ok(self)
    }

    pub fn init(&self) -> Result<(), DynLogAPIErr> {
        let global = &self.config.global;
        if global.options.enabled {
            let stream_targets = Targets::from_str(&self.config.stream_logger.modules.join(","))
                .map(|targets| {
                    targets
                        .into_iter()
                        .map(|(filter, _)| (filter, LevelFilter::OFF))
                        .collect::<Targets>()
                })?;

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
            if stream_targets.iter().count() > 0 {
                ref_layers.push(
                    env_layer
                        .with_filter(
                            stream_targets.with_default(LevelFilter::from_level(global.log_level)),
                        )
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

        Ok(())
    }

    fn register_filelogger_target(&self, entry: &FileLogger) -> Result<(), DynLogAPIErr> {
        let log_dir = &entry.path;
        create_dir_all(log_dir).map_err(|source| DynLogAPIErr::CreateLogDirError {
            path: log_dir.to_string_lossy().to_string(),
            source,
        })?;
        let appender = tracing_appender::rolling::never(log_dir, &entry.filename);
        let (file_writer, guard) = tracing_appender::non_blocking(appender);
        self.guards.borrow_mut().push(guard);

        let options = &entry.options;
        let file_targets = Targets::from_str(&entry.modules.join(","))?;
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
    ) -> Result<Self, DynLogAPIErr> {
        let target_layer = {
            let file_targets = Targets::from_str(&self.config.stream_logger.modules.join(","))?;
            if file_targets.iter().count() > 0 {
                layer.with_filter(file_targets).boxed()
            } else {
                layer
            }
        };

        self.layers.borrow_mut().push(target_layer);

        Ok(self)
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
    type Error;
    fn init_stdout(&self) -> Result<(), Self::Error>;
    fn init_filelogger(&self) -> Result<(), Self::Error>;
}

impl DynamicLogging for DynamicLogger {
    type Error = DynLogAPIErr;
    fn init_stdout(&self) -> Result<(), Self::Error> {
        let stream_logger = &self.config.stream_logger;
        if !stream_logger.options.enabled {
            return Ok(());
        }

        let targets = Targets::from_str(&stream_logger.modules.join(",")).expect("");
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

    fn init_filelogger(&self) -> Result<(), Self::Error> {
        let file_logger_table = &self
            .config
            .file_logger
            .as_ref()
            .ok_or(DynLogAPIErr::InitializeFileloggerError)?;
        for entry in file_logger_table.iter().filter(|file| file.options.enabled) {
            self.register_filelogger_target(entry)?;
        }
        Ok(())
    }
}
