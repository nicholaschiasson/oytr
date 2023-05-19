use std::fmt::Display;
use std::ops::{Deref, DerefMut};
use std::path::PathBuf;
use std::str::FromStr;
use std::{error, process};

use chrono::{DateTime, Local};
use clap::{Args, Parser, Subcommand};
use cron::Schedule;
use lazy_static::lazy_static;
use notify_rust::Notification;
use serde::de::Visitor;
use serde::{Deserialize, Serialize};

#[derive(Subcommand)]
enum OytrCommand {
    /// Add reminder
    Add(Reminder),
    /// List reminders
    List,
    /// Remove reminder
    Remove {
        /// ID of reminder to remove, retrievable through `list` subcommand
        id: usize,
    },
}

#[derive(Parser)]
#[command(about, author, version, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<OytrCommand>,
    /// Path to config file
    #[arg(short, long, value_name = "FILE", default_value = DEFAULT_CONFIGURATION_FILE_PATH.as_str())]
    config: PathBuf,
}

struct CronScheduleVisitor;

impl<'de> Visitor<'de> for CronScheduleVisitor {
    type Value = CronSchedule;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a cron schedule expression")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Self::Value::from_str(v).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug)]
struct CronSchedule {
    schedule: Schedule,
}

impl Deref for CronSchedule {
    type Target = Schedule;

    fn deref(&self) -> &Self::Target {
        &self.schedule
    }
}

impl DerefMut for CronSchedule {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.schedule
    }
}

impl Display for CronSchedule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.schedule)
    }
}

impl From<CronSchedule> for String {
    fn from(value: CronSchedule) -> Self {
        value.to_string()
    }
}

impl FromStr for CronSchedule {
    type Err = cron::error::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self {
            schedule: Schedule::from_str(s)?,
        })
    }
}

impl<'de> Deserialize<'de> for CronSchedule {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_str(CronScheduleVisitor)
    }
}

impl Serialize for CronSchedule {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

#[derive(Args, Clone, Debug, Deserialize, Serialize)]
struct Reminder {
    #[arg(skip)]
    id: Option<usize>,
    /// Reminder summary line
    summary: String,
    /// Reminder description
    description: String,
    /// Reminder cron schedule expression
    schedule: CronSchedule,
    #[arg(skip)]
    #[serde(skip)]
    upcoming: Option<DateTime<Local>>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct Config {
    reminders: Vec<Reminder>,
}

lazy_static! {
    static ref DEFAULT_CONFIGURATION_FILE_PATH: String =
        confy::get_configuration_file_path("oytr", None)
            .expect("default configuration file path")
            .display()
            .to_string();
}

fn main() -> Result<(), Box<dyn error::Error>> {
    let cli = Cli::parse();

    let mut cfg: Config = confy::load_path(cli.config.clone())?;

    match cli.command {
        Some(OytrCommand::Add(reminder)) => {
            println!("Adding reminder:");
            println!("{}", toml::to_string(&reminder)?);
            cfg.reminders.push(reminder);
            confy::store_path(cli.config, cfg)?;
        }
        Some(OytrCommand::List) => {
            println!(
                "{}",
                toml::to_string(&Config {
                    reminders: cfg
                        .reminders
                        .iter()
                        .enumerate()
                        .map(move |(i, r)| Reminder {
                            id: Some(i),
                            ..r.clone()
                        })
                        .collect::<Vec<_>>()
                })?
            );
        }
        Some(OytrCommand::Remove { id }) => {
            println!("Removing reminder:");
            println!("{}", toml::to_string(&cfg.reminders.remove(id))?);
            confy::store_path(cli.config, cfg)?;
        }
        None => {
            ctrlc::set_handler(|| process::exit(0))?;
            loop {
                for reminder in cfg.reminders.iter_mut() {
                    let schedule = (*reminder.schedule).upcoming(Local).nth(1);
                    if reminder.upcoming.is_none() {
                        reminder.upcoming = schedule;
                    } else if reminder.upcoming != schedule {
                        reminder.upcoming = schedule;
                        println!(
                            "New notification: {} - {}",
                            reminder.summary, reminder.description
                        );
                        Notification::new()
                            .summary(&reminder.summary)
                            .body(&reminder.description)
                            .show()?;
                    }
                }
            }
        }
    }

    Ok(())
}
