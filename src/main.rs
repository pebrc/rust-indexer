extern crate chrono;
#[macro_use]
extern crate lazy_static;
extern crate notify;
extern crate regex;

use notify::{DebouncedEvent, RecommendedWatcher, RecursiveMode, Watcher};
use notify::DebouncedEvent::*;
use std::sync::mpsc::channel;
use std::time::Duration;
use std::path::PathBuf;
use std::fs::{create_dir_all, remove_file};
use std::os::unix::fs::symlink;
use std::io;
use regex::Regex;
use chrono::prelude::*;

fn matches(path: &PathBuf) -> Option<String> {
    lazy_static! {
        static ref RE: Regex = Regex::new("([0-9]{8})[^0-9]+").unwrap(); //accept panic here
    }
    path.to_str()
        .and_then(|str| RE.captures(str))
        .and_then(|cap| cap.get(1))
        .map(|m| String::from(m.as_str()))
}

fn parse_date(path: String) -> Result<NaiveDate, chrono::ParseError> {
    NaiveDate::parse_from_str(&path, "%Y%m%d")
        .or_else(|_| NaiveDate::parse_from_str(&path, "%d%m%Y"))
}

fn target_path(target: &str, date: NaiveDate, path: &PathBuf) -> Option<PathBuf> {
    path.file_name()
        .and_then(|os_str| os_str.to_str())
        .map(|file| {
            let mut pb = PathBuf::from(target);
            pb.push(date.year().to_string().as_str());
            pb.push(date.month().to_string().as_str());
            pb.push(date.day().to_string().as_str());
            pb.push(file);
            pb
        })
}

fn linker<F>(target: &str, date_str: String, path: &PathBuf, op: F) -> io::Result<()>
where
    F: FnOnce(&PathBuf) -> io::Result<()>,
{
    parse_date(date_str)
        .map_err(|pe| io::Error::new(io::ErrorKind::Other, pe.to_string()))
        .and_then(|d| {
            target_path(target, d, path)
                .ok_or(io::Error::new(io::ErrorKind::Other, "no target path"))
        })
        .and_then(|index_path| {
            let target_path = index_path.as_path();
            let target_dir = target_path.parent();
            match target_dir {
                Some(dir) if (!dir.exists()) => {
                    println!("Trying to create {:?}", target_dir);
                    create_dir_all(dir)
                }
                _ => Ok(()),
            }.and_then(|_| {
                if target_path.exists() {
                    println!("Trying to remove {:?}", target_path);
                    remove_file(target_path)
                } else {
                    Ok(())
                }
            })
                .and_then(|_| Ok(&index_path))
                .and_then(op)
        })
}

fn link(target: &str, date_str: String, path: &PathBuf) -> io::Result<()> {
    linker(target, date_str, path, |target_path| {
        println!("Symlinking {:?} {:?}", path, target_path);
        symlink(path, target_path)
    })
}

fn unlink(target: &str, date_str: String, path: &PathBuf) -> io::Result<()> {
    linker(target, date_str, path, |_| Ok(()))
}

fn handle<'a>(target: &str, evt: &'a DebouncedEvent) -> &'a DebouncedEvent {
    println!(
        "{:?}",
        match evt {
            &Create(ref path) => matches(&path).map(|s| link(target, s, &path)),
            &Write(ref path) => matches(&path).map(|s| link(target, s, &path)),
            &Chmod(ref path) => matches(&path).map(|s| link(target, s, &path)),
            &Remove(ref path) => matches(&path).map(|s| unlink(target, s, &path)),
            &Rename(_, ref to) => matches(&to).map(|s| {
                //unlink(target, s, &from)
                link(target, s, &to)
            }),
            _ => None,
        }
    );
    evt
}

fn index_to(target: &str) -> notify::Result<()> {
    // Create a channel to receive the events.
    let (tx, rx) = channel();

    // Automatically select the best implementation for your platform.
    // You can also access each implementation directly e.g. INotifyWatcher.
    let mut watcher: RecommendedWatcher = try!(Watcher::new(tx, Duration::from_secs(2)));

    // Add a path to be watched. All files and directories at that path and
    // below will be monitored for changes.
    try!(watcher.watch("/tmp/foo", RecursiveMode::Recursive));

    loop {
        match rx.recv() {
            Ok(event) => println!("{:?}", handle(target, &event)),
            Err(e) => println!("watch error: {:?}", e),
        }
    }
}

fn main() {
    let target = "/tmp/index";
    if let Err(e) = index_to(&target) {
        println!("error: {:?}", e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parsing_dates() {
        let dt = NaiveDate::from_ymd(2014, 11, 28);
        assert_eq!(parse_date(String::from("20141128")), Ok(dt.clone()));
        assert_eq!(parse_date(String::from("28112014")), Ok(dt.clone()));
    }
}
