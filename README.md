# Dyn Logger

This crate provides a configuration option for the tracing logging crate,
allowing you to configure the logging based on a `logging.toml` file. With this
file, you can easily change the log level per module, as well as the format and
other options for the logger. Additionally, the crate allows you to configure
multiple file loggers, each with its own log level, format, color, and path
options.

## Usage

To use this crate, simply add it as a dependency to your Cargo.toml file:

```toml
[dependencies]
dyn-logger = "1.0"
```

Then, configure your logger as shown below. This will read the logging.toml
file in your project's root directory, and apply the configuration settings to
your tracing logger.

```rust
fn main() -> Result<(), Box<dyn std::error::Error>>  {
    let logger = DynamicLogger::new("path/to/logging.toml")?
        .with_stdout()?
        .with_file_logger()?;
    logger.init()?;

    // ...

    Ok(())
}
```

## Configuration

The logging.toml file allows you to configure the logging options for your
application. Here is an example:

```toml
# global logging options
[global]
log_level = "trace"
enabled = false
format = "compact" # Options: `full`, `compact`, `pretty` and `json`
line_number = true
file = true
thread_name = true
thread_id = true

 # special std out logging, overrides global settings
[stream_logger]
enabled = true
color = true
format = "pretty" # Options: `full`, `compact`, `pretty` and `json`
line_number = true
file = true
thread_name = true
thread_id = true
modules = [
    "module=trace",
    "module::submodule_one=trace",
    "module::submodule_two=debug",
    "module::submodule_three=info",
]

# logging to files, uses global options
[[file_logger]]
enabled = true
filename = "logfile.log"
path = "./logs"
line_number = false # override global options
thread_id = false # override global options
format = "json" # Options: `full`, `compact`, `pretty` and `json`
modules = [
    "module_name::submodule::example=trace",
]

[[file_logger]]
enabled = true
filename = "another_logfile.log"
path = "./logs"
format = "compact" # Options: `full`, `compact`, `pretty` and `json`
modules = [
    "module_name::submodule=trace",
    "module_name::other_submodule=debug",
]
```

# License

This crate is licensed under the EUPL-1.2 License. See the
[LICENSE](./LICENSE.md) file for more information.
