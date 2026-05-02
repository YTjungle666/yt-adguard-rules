use chrono::Local;
use regex::Regex;
use reqwest::blocking::Client;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::thread;
use std::time::Duration;

static ADBLOCK_DOMAIN_RULE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(@@)?\|\|([^\^/$|]+)(\^.*)?$").unwrap());
static HOST_DOMAIN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:[0-9.]+\s+)?([A-Za-z0-9*._-]+\.[A-Za-z0-9._-]+)$").unwrap());

#[derive(Clone, Debug, Eq, PartialEq)]
struct DomainRule {
    exception: bool,
    domain: String,
    suffix: String,
}

#[derive(Debug, Default)]
struct DomainCoverage {
    broad_domains: HashMap<bool, HashSet<String>>,
    suffix_domains: HashMap<(bool, String), HashSet<String>>,
}

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

    let merged_allow = compact_domain_rules(allow.into_iter().collect());
    let allow_coverage = DomainCoverage::from_rules(&merged_allow);

    let mut block_rules: Vec<String> = block.into_iter().collect();
    block_rules.sort_unstable();

    let block_rules: Vec<String> = block_rules
        .into_iter()
        .filter(|rule| !allow_coverage.covers_block_rule(rule))
        .collect();
    let merged_block = compact_domain_rules(block_rules);

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
    if let Some(domain_rule) = adblock_domain_rule(rule) {
        return Some(domain_rule.domain);
    }

    host_domain_key(rule)
}

fn host_domain_key(rule: &str) -> Option<String> {
    HOST_DOMAIN
        .captures(rule.trim())
        .and_then(|captures| captures.get(1).map(|m| normalize_domain(m.as_str())))
}

fn adblock_domain_rule(rule: &str) -> Option<DomainRule> {
    let captures = ADBLOCK_DOMAIN_RULE.captures(rule.trim())?;
    let domain = captures.get(2)?.as_str();
    Some(DomainRule {
        exception: captures.get(1).is_some(),
        domain: normalize_domain(domain),
        suffix: captures
            .get(3)
            .map_or_else(|| "^".to_string(), |m| m.as_str().to_string()),
    })
}

fn normalize_domain(domain: &str) -> String {
    domain.trim_end_matches('.').to_ascii_lowercase()
}

fn compact_domain_rules(mut rules: Vec<String>) -> Vec<String> {
    rules.sort_unstable();
    let coverage = DomainCoverage::from_rules(&rules);

    rules
        .into_iter()
        .filter(|rule| {
            adblock_domain_rule(rule)
                .as_ref()
                .is_none_or(|domain_rule| !coverage.has_covering_parent(domain_rule))
        })
        .collect()
}

impl DomainCoverage {
    fn from_rules(rules: &[String]) -> Self {
        let mut coverage = Self::default();
        for rule in rules {
            if let Some(domain_rule) = adblock_domain_rule(rule) {
                coverage.insert(domain_rule);
            }
        }
        coverage
    }

    fn insert(&mut self, rule: DomainRule) {
        if rule.is_broad() {
            self.broad_domains
                .entry(rule.exception)
                .or_default()
                .insert(rule.domain.clone());
        }
        self.suffix_domains
            .entry((rule.exception, rule.suffix))
            .or_default()
            .insert(rule.domain);
    }

    fn covers_block_rule(&self, rule: &str) -> bool {
        if let Some(domain_rule) = adblock_domain_rule(rule) {
            return !domain_rule.exception
                && self.has_covering_domain(true, &domain_rule.domain, &domain_rule.suffix, true);
        }

        host_domain_key(rule)
            .as_ref()
            .is_some_and(|domain| self.has_broad_covering_domain(true, domain))
    }

    fn has_covering_parent(&self, rule: &DomainRule) -> bool {
        parent_domains(&rule.domain)
            .any(|parent| self.domain_covers_with_suffix(rule.exception, parent, &rule.suffix))
    }

    fn has_covering_domain(
        &self,
        exception: bool,
        domain: &str,
        suffix: &str,
        include_self: bool,
    ) -> bool {
        if include_self && self.domain_covers_with_suffix(exception, domain, suffix) {
            return true;
        }
        parent_domains(domain)
            .any(|parent| self.domain_covers_with_suffix(exception, parent, suffix))
    }

    fn has_broad_covering_domain(&self, exception: bool, domain: &str) -> bool {
        self.domain_has_broad_rule(exception, domain)
            || parent_domains(domain).any(|parent| self.domain_has_broad_rule(exception, parent))
    }

    fn domain_covers_with_suffix(&self, exception: bool, domain: &str, suffix: &str) -> bool {
        self.domain_has_broad_rule(exception, domain)
            || self
                .suffix_domains
                .get(&(exception, suffix.to_string()))
                .is_some_and(|domains| domains.contains(domain))
    }

    fn domain_has_broad_rule(&self, exception: bool, domain: &str) -> bool {
        self.broad_domains
            .get(&exception)
            .is_some_and(|domains| domains.contains(domain))
    }
}

impl DomainRule {
    fn is_broad(&self) -> bool {
        self.suffix == "^" || self.suffix == "^$important"
    }
}

fn parent_domains(domain: &str) -> ParentDomains<'_> {
    ParentDomains { domain }
}

struct ParentDomains<'a> {
    domain: &'a str,
}

impl<'a> Iterator for ParentDomains<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        let idx = self.domain.find('.')?;
        self.domain = &self.domain[idx + 1..];
        Some(self.domain)
    }
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
    verify_no_covered_subdomains("allowlist", merged_allow)?;
    verify_no_covered_subdomains("blocklist", merged_block)?;

    let allow_coverage = DomainCoverage::from_rules(merged_allow);

    let overlap: Vec<(String, String)> = merged_block
        .iter()
        .filter_map(|rule| {
            let key = domain_key(rule)?;
            allow_coverage
                .covers_block_rule(rule)
                .then(|| (rule.clone(), key))
        })
        .take(20)
        .collect();

    if !overlap.is_empty() {
        eprintln!("ERROR: overlap remains {overlap:?}");
        return Err(io::Error::other("allowlist/blocklist overlap remains").into());
    }

    Ok(())
}

fn verify_no_covered_subdomains(
    name: &str,
    rules: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    let coverage = DomainCoverage::from_rules(rules);
    let redundant: Vec<(String, String)> = rules
        .iter()
        .filter_map(|rule| {
            let domain_rule = adblock_domain_rule(rule)?;
            coverage
                .has_covering_parent(&domain_rule)
                .then(|| (rule.clone(), domain_rule.domain))
        })
        .take(20)
        .collect();

    if !redundant.is_empty() {
        eprintln!("ERROR: {name} redundant subdomain rules remain {redundant:?}");
        return Err(io::Error::other(format!("{name} redundant subdomain rules remain")).into());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(items: &[&str]) -> Vec<String> {
        items.iter().map(|rule| rule.to_string()).collect()
    }

    #[test]
    fn compact_domain_rules_keeps_parent_and_drops_covered_subdomains() {
        assert_eq!(
            compact_domain_rules(rules(&[
                "@@||example.com^$important",
                "@@||a.example.com^$important",
                "@@||b.a.example.com^$important",
                "@@||other.com^$script",
                "@@||x.other.com^$script",
                "@@||keep.other.com^$image",
            ])),
            rules(&[
                "@@||example.com^$important",
                "@@||keep.other.com^$image",
                "@@||other.com^$script",
            ])
        );
    }

    #[test]
    fn allow_coverage_removes_blocked_subdomains() {
        let allow = compact_domain_rules(rules(&[
            "@@||zijieapi.com^$important",
            "@@||mssdk3-normal-hj.zijieapi.com^$important",
            "@@||example.com^$script",
        ]));
        let coverage = DomainCoverage::from_rules(&allow);

        assert_eq!(
            allow,
            rules(&["@@||example.com^$script", "@@||zijieapi.com^$important"])
        );
        assert!(coverage.covers_block_rule("||zijieapi.com^"));
        assert!(coverage.covers_block_rule("||mssdk3-normal-hj.zijieapi.com^"));
        assert!(coverage.covers_block_rule("||ads*-normal-lf.zijieapi.com^"));
        assert!(coverage.covers_block_rule("||cdn.example.com^$script"));
        assert!(!coverage.covers_block_rule("||cdn.example.com^$image"));
        assert!(!coverage.covers_block_rule("||zijieapi.com.c.dsa.cdnbuild.net^"));
    }

    #[test]
    fn host_rules_are_not_compacted_as_parent_domains() {
        assert_eq!(
            compact_domain_rules(rules(&["example.com", "sub.example.com"])),
            rules(&["example.com", "sub.example.com"])
        );
    }
}
