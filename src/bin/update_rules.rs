use chrono::Local;
use regex::Regex;
use reqwest::blocking::Client;
use serde::Deserialize;
use std::collections::HashSet;
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::thread;
use std::time::Duration;

static ADBLOCK_DOMAIN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\|\|([^\^/$|]+)").unwrap());
static HOST_DOMAIN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:[0-9.]+\s+)?([A-Za-z0-9*._-]+\.[A-Za-z0-9._-]+)$").unwrap());

#[derive(Debug, Deserialize)]
struct Sources {
    filters: Vec<Source>,
    whitelists: Vec<Source>,
}

#[derive(Debug, Deserialize)]
struct Source(String, String);

#[derive(Clone, Copy, Debug)]
enum SourceKind {
    Filter,
    Whitelist,
}

#[derive(Clone, Debug)]
struct FetchTask {
    kind: SourceKind,
    name: String,
    url: String,
    timeout: Duration,
}

#[derive(Debug)]
struct FetchedSource {
    kind: SourceKind,
    text: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let base = repo_base()?;
    let sources: Sources = serde_json::from_str(&fs::read_to_string(base.join("sources.json"))?)?;

    let mut allow = load_rule_file(&base.join("custom-allowlist.txt"))?;
    let mut block = load_rule_file(&base.join("custom-blocklist.txt"))?;

    let fetched = fetch_sources(&sources)?;
    for src in fetched {
        for line in src.text.lines() {
            let s = line.trim();
            if !valid(s) {
                continue;
            }

            match src.kind {
                SourceKind::Whitelist => {
                    if s.starts_with("@@") {
                        allow.insert(s.to_string());
                    } else {
                        allow.insert(format!("@@{s}"));
                    }
                }
                SourceKind::Filter => {
                    if s.starts_with("@@") {
                        allow.insert(s.to_string());
                    } else {
                        block.insert(s.to_string());
                    }
                }
            }
        }
    }

    let mut merged_allow: Vec<String> = allow.into_iter().collect();
    merged_allow.sort_unstable();

    let allow_keys: HashSet<String> = merged_allow.iter().filter_map(|r| domain_key(r)).collect();

    let mut block_rules: Vec<String> = block.into_iter().collect();
    block_rules.sort_unstable();

    let mut merged_block = Vec::with_capacity(block_rules.len());
    for rule in block_rules {
        if domain_key(&rule)
            .as_ref()
            .is_some_and(|key| allow_keys.contains(key))
        {
            continue;
        }
        merged_block.push(rule);
    }

    write_outputs(&base, &merged_block, &merged_allow)?;
    verify_no_overlap(&merged_block, &merged_allow)?;

    println!(
        "OK block={} allow={}",
        merged_block.len(),
        merged_allow.len()
    );

    Ok(())
}

fn repo_base() -> io::Result<PathBuf> {
    let cwd = env::current_dir()?;
    if cwd.join("sources.json").is_file() {
        return Ok(cwd);
    }

    let mut dir = env::current_exe()?.canonicalize()?;
    while dir.pop() {
        if dir.join("sources.json").is_file() {
            return Ok(dir);
        }
    }

    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "could not find repository root containing sources.json",
    ))
}

fn fetch_sources(sources: &Sources) -> Result<Vec<FetchedSource>, Box<dyn std::error::Error>> {
    let client = Client::builder()
        .user_agent("yt-adguard-rules/0.1")
        .build()?;

    let mut tasks = Vec::with_capacity(sources.whitelists.len() + sources.filters.len());
    for Source(name, url) in &sources.whitelists {
        tasks.push(FetchTask {
            kind: SourceKind::Whitelist,
            name: name.clone(),
            url: url.clone(),
            timeout: Duration::from_secs(120),
        });
    }
    for Source(name, url) in &sources.filters {
        tasks.push(FetchTask {
            kind: SourceKind::Filter,
            name: name.clone(),
            url: url.clone(),
            timeout: Duration::from_secs(180),
        });
    }

    let mut handles = Vec::with_capacity(tasks.len());
    for task in tasks {
        let client = client.clone();
        handles.push(thread::spawn(move || fetch_source(client, task)));
    }

    let mut fetched = Vec::with_capacity(handles.len());
    let mut errors = Vec::new();
    for handle in handles {
        match handle.join() {
            Ok(Ok(source)) => fetched.push(source),
            Ok(Err(err)) => errors.push(err),
            Err(_) => errors.push("source download thread panicked".to_string()),
        }
    }

    if !errors.is_empty() {
        for err in &errors {
            eprintln!("{err}");
        }
        return Err(io::Error::other(format!("{} source(s) failed", errors.len())).into());
    }

    Ok(fetched)
}

fn fetch_source(client: Client, task: FetchTask) -> Result<FetchedSource, String> {
    let response = client
        .get(&task.url)
        .timeout(task.timeout)
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(|err| format!("failed to fetch {} ({}): {err}", task.name, task.url))?;

    let text = response
        .text()
        .map_err(|err| format!("failed to read {} ({}): {err}", task.name, task.url))?;

    Ok(FetchedSource {
        kind: task.kind,
        text,
    })
}

fn load_rule_file(path: &Path) -> io::Result<HashSet<String>> {
    let mut rules = HashSet::new();
    for line in fs::read_to_string(path)?.lines() {
        let s = line.trim();
        if valid(s) {
            rules.insert(s.to_string());
        }
    }
    Ok(rules)
}

fn valid(line: &str) -> bool {
    let s = line.trim();
    if s.is_empty() || s.starts_with('!') || s.starts_with('#') || s.starts_with('[') {
        return false;
    }
    if (s.starts_with("$removeparam") || s.starts_with('/'))
        && !s.starts_with("@@||")
        && !s.starts_with("||")
    {
        return false;
    }
    true
}

fn domain_key(rule: &str) -> Option<String> {
    let mut r = rule.trim();
    if let Some(stripped) = r.strip_prefix("@@") {
        r = stripped;
    }

    if let Some(captures) = ADBLOCK_DOMAIN.captures(r) {
        return captures.get(1).map(|m| m.as_str().to_string());
    }

    HOST_DOMAIN
        .captures(r)
        .and_then(|captures| captures.get(1).map(|m| m.as_str().to_string()))
}

fn write_outputs(base: &Path, merged_block: &[String], merged_allow: &[String]) -> io::Result<()> {
    let today = Local::now().date_naive();

    fs::write(
        base.join("blocklist.txt"),
        format!(
            "! YT merged AdGuard blocklist\n\
             ! Generated: {today}\n\
             ! Sources: enabled upstream blocklists + upstream allow exceptions + custom rules\n\n\
             {}\n",
            merged_block.join("\n")
        ),
    )?;

    fs::write(
        base.join("allowlist.txt"),
        format!(
            "! YT merged AdGuard allowlist\n\
             ! Generated: {today}\n\
             ! Sources: enabled upstream allowlists + custom allow rules + upstream allow exceptions\n\n\
             {}\n",
            merged_allow.join("\n")
        ),
    )?;

    Ok(())
}

fn verify_no_overlap(
    merged_block: &[String],
    merged_allow: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    let block_keys: HashSet<String> = merged_block.iter().filter_map(|r| domain_key(r)).collect();
    let overlap: Vec<(String, String)> = merged_allow
        .iter()
        .filter_map(|rule| {
            let key = domain_key(rule)?;
            block_keys.contains(&key).then(|| (rule.clone(), key))
        })
        .take(20)
        .collect();

    if !overlap.is_empty() {
        eprintln!("ERROR: overlap remains {overlap:?}");
        return Err(io::Error::other("allowlist/blocklist overlap remains").into());
    }

    Ok(())
}
