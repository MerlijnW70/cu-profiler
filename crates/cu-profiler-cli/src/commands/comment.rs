//! `cu-profiler comment` — post the Markdown report as a sticky pull-request comment.
//!
//! In CI the report is delivered as a single "sticky" comment: created once, then
//! updated in place on every later run, so a PR accrues one always-current report
//! instead of a new comment per push. The comment is found again by a hidden HTML
//! marker (`<!-- cu-profiler-report -->`) embedded as its first line.
//!
//! Networking reuses the same `remote`-feature `ureq` (rustls, openssl-free) stack
//! as `import --signature`. All decision logic — marker handling, repo/PR/event
//! parsing, choosing update-vs-create — is pure and unit-tested; only the GitHub
//! REST calls live behind `#[cfg(feature = "remote")]`.

use cu_profiler_core::{Error, Result};

use crate::args::CommentArgs;
use crate::commands::{MAX_LOG_BYTES, read_to_string_capped};
use crate::exit::ExitCode;

/// GitHub rejects issue/PR comment bodies longer than 65,536 characters.
const GITHUB_MAX_BODY: usize = 65_536;

/// The hidden marker line that identifies our sticky comment.
fn marker_line(marker: &str) -> String {
    format!("<!-- {marker} -->")
}

/// Prefix `markdown` with the hidden marker and truncate to GitHub's body limit.
fn sticky_body(marker: &str, markdown: &str) -> String {
    let mut body = marker_line(marker);
    body.push('\n');
    body.push_str(markdown);
    truncate_to_limit(body)
}

/// Truncate a body to [`GITHUB_MAX_BODY`] characters, appending a visible note so a
/// cut-off report is never silently presented as complete.
fn truncate_to_limit(body: String) -> String {
    if body.chars().count() <= GITHUB_MAX_BODY {
        return body;
    }
    let note = "\n\n_…report truncated to fit GitHub's comment size limit._";
    let budget = GITHUB_MAX_BODY.saturating_sub(note.chars().count());
    let mut truncated: String = body.chars().take(budget).collect();
    truncated.push_str(note);
    truncated
}

/// Split an `owner/repo` slug, rejecting anything malformed.
fn split_repo(slug: &str) -> Result<(String, String)> {
    let invalid = || {
        Error::Config(format!(
            "invalid repository `{slug}` — expected `owner/repo`"
        ))
    };
    let (owner, repo) = slug.split_once('/').ok_or_else(invalid)?;
    if owner.is_empty() || repo.is_empty() || repo.contains('/') {
        return Err(invalid());
    }
    Ok((owner.to_string(), repo.to_string()))
}

/// Extract a PR number from a GitHub Actions event payload (`pull_request.number`,
/// a top-level `number`, or `issue.number`).
fn pr_from_event_json(v: &serde_json::Value) -> Option<u64> {
    let as_num = serde_json::Value::as_u64;
    v.get("pull_request")
        .and_then(|p| p.get("number"))
        .and_then(as_num)
        .or_else(|| v.get("number").and_then(as_num))
        .or_else(|| {
            v.get("issue")
                .and_then(|i| i.get("number"))
                .and_then(as_num)
        })
}

/// Extract a PR number from a `refs/pull/<n>/merge` ref.
fn pr_from_ref(git_ref: &str) -> Option<u64> {
    git_ref
        .strip_prefix("refs/pull/")?
        .split('/')
        .next()?
        .parse()
        .ok()
}

/// Resolve the target `owner/repo` from `--repo` or `$GITHUB_REPOSITORY`.
fn resolve_repo(args: &CommentArgs) -> Result<(String, String)> {
    let slug = args
        .repo
        .clone()
        .or_else(|| std::env::var("GITHUB_REPOSITORY").ok())
        .ok_or_else(|| {
            Error::Config(
                "no repository — pass `--repo owner/repo` or run under GitHub Actions \
                 (which sets $GITHUB_REPOSITORY)"
                    .to_string(),
            )
        })?;
    split_repo(&slug)
}

/// Resolve the PR number from `--pr`, then the Actions event payload, then the ref.
/// `None` means "no PR in scope" (e.g. a `push` build) — the caller no-ops.
fn resolve_pr(args: &CommentArgs) -> Option<u64> {
    if let Some(pr) = args.pr {
        return Some(pr);
    }
    if let Some(pr) = std::env::var("GITHUB_EVENT_PATH")
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|v| pr_from_event_json(&v))
    {
        return Some(pr);
    }
    std::env::var("GITHUB_REF")
        .ok()
        .and_then(|r| pr_from_ref(&r))
}

/// Build the Markdown body: from `--input`, else by re-rendering from config.
fn render_body(args: &CommentArgs) -> Result<String> {
    if let Some(path) = &args.input {
        return read_to_string_capped(path, MAX_LOG_BYTES);
    }
    let loaded = super::load_config(&args.common.config)?;
    let (report, _scenarios, _baseline) = super::profile(&loaded, &args.common, None)?;
    cu_profiler_report::render(&report, cu_profiler_report::Format::Markdown)
}

/// Execute the `comment` command.
pub fn run(args: &CommentArgs, quiet: bool) -> Result<ExitCode> {
    let markdown = render_body(args)?;
    let body = sticky_body(&args.marker, &markdown);

    if args.dry_run {
        print!("{body}");
        if !body.ends_with('\n') {
            println!();
        }
        return Ok(ExitCode::Success);
    }

    let pr = match resolve_pr(args) {
        Some(pr) => pr,
        None => {
            if !quiet {
                eprintln!("note: no pull request in scope (not a PR event) — skipping comment.");
            }
            return Ok(ExitCode::Success);
        }
    };
    let (owner, repo) = resolve_repo(args)?;
    post_comment(&owner, &repo, pr, &args.marker, &body, quiet)
}

/// Post or update the sticky comment via the GitHub REST API.
#[cfg(feature = "remote")]
fn post_comment(
    owner: &str,
    repo: &str,
    pr: u64,
    marker: &str,
    body: &str,
    quiet: bool,
) -> Result<ExitCode> {
    let token = std::env::var("GITHUB_TOKEN").map_err(|_| {
        Error::Config(
            "no GITHUB_TOKEN in the environment — grant the workflow \
             `permissions: pull-requests: write` and pass `${{ secrets.GITHUB_TOKEN }}`"
                .to_string(),
        )
    })?;
    let agent = github_agent();
    let existing = find_sticky_comment(&agent, owner, repo, pr, &token, marker)?;
    let payload = serde_json::json!({ "body": body });

    let url = match existing {
        Some(id) => format!("https://api.github.com/repos/{owner}/{repo}/issues/comments/{id}"),
        None => format!("https://api.github.com/repos/{owner}/{repo}/issues/{pr}/comments"),
    };
    let request = match existing {
        Some(_) => agent.patch(&url),
        None => agent.post(&url),
    };
    authed(request, &token)
        .send_json(&payload)
        .map_err(|e| Error::Simulation(format!("GitHub comment request failed: {e}")))?;

    if !quiet {
        let verb = if existing.is_some() {
            "updated"
        } else {
            "posted"
        };
        println!("{verb} sticky report comment on {owner}/{repo}#{pr}");
    }
    Ok(ExitCode::Success)
}

/// A `ureq` agent with a short global timeout, matching `import`'s configuration.
#[cfg(feature = "remote")]
fn github_agent() -> ureq::Agent {
    use std::time::Duration;
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(20)))
        .build();
    config.into()
}

/// Apply the headers GitHub requires on every REST call. Generic over `ureq`'s
/// body typestate so it works for GET (no body) and POST/PATCH (with body) alike.
#[cfg(feature = "remote")]
fn authed<B>(builder: ureq::RequestBuilder<B>, token: &str) -> ureq::RequestBuilder<B> {
    builder
        .header("Authorization", &format!("Bearer {token}"))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header(
            "User-Agent",
            concat!("cu-profiler/", env!("CARGO_PKG_VERSION")),
        )
}

/// Find this tool's sticky comment by scanning every page of PR comments for the
/// hidden marker. Returns its comment id, or `None` to create a new one.
#[cfg(feature = "remote")]
fn find_sticky_comment(
    agent: &ureq::Agent,
    owner: &str,
    repo: &str,
    pr: u64,
    token: &str,
    marker: &str,
) -> Result<Option<u64>> {
    for page in 1..=100u32 {
        let url = format!(
            "https://api.github.com/repos/{owner}/{repo}/issues/{pr}/comments?per_page=100&page={page}"
        );
        let mut resp = authed(agent.get(&url), token)
            .call()
            .map_err(|e| Error::Simulation(format!("GitHub comment lookup failed: {e}")))?;
        let comments: Vec<serde_json::Value> = resp
            .body_mut()
            .with_config()
            .limit(MAX_LOG_BYTES)
            .read_json()
            .map_err(|e| Error::Simulation(format!("invalid GitHub response: {e}")))?;
        if let Some(id) = find_in_page(&comments, marker) {
            return Ok(Some(id));
        }
        if comments.len() < 100 {
            break;
        }
    }
    Ok(None)
}

/// The id of the first comment in `comments` whose body carries `marker` (pure).
#[cfg(feature = "remote")]
fn find_in_page(comments: &[serde_json::Value], marker: &str) -> Option<u64> {
    let needle = marker_line(marker);
    comments.iter().find_map(|c| {
        let body = c.get("body")?.as_str()?;
        body.contains(&needle)
            .then(|| c.get("id").and_then(serde_json::Value::as_u64))
            .flatten()
    })
}

/// Without the `remote` feature there is no HTTP stack to post with.
#[cfg(not(feature = "remote"))]
fn post_comment(
    _owner: &str,
    _repo: &str,
    _pr: u64,
    _marker: &str,
    _body: &str,
    _quiet: bool,
) -> Result<ExitCode> {
    Err(Error::Config(
        "posting PR comments requires the `remote` feature (on by default); rebuild with \
         `--features remote`, or use `--dry-run` to render the body without posting"
            .to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sticky_body_prefixes_the_marker() {
        let body = sticky_body("cu-profiler-report", "## report\nok");
        assert!(body.starts_with("<!-- cu-profiler-report -->\n"));
        assert!(body.contains("## report"));
    }

    #[test]
    fn truncate_caps_oversized_bodies_with_a_note() {
        let huge = "x".repeat(GITHUB_MAX_BODY + 5_000);
        let out = truncate_to_limit(huge);
        assert!(out.chars().count() <= GITHUB_MAX_BODY);
        assert!(out.contains("truncated"));
    }

    #[test]
    fn small_bodies_are_untouched() {
        let small = "tiny".to_string();
        assert_eq!(truncate_to_limit(small.clone()), small);
    }

    #[test]
    fn split_repo_parses_owner_and_repo() {
        assert_eq!(
            split_repo("MerlijnW70/cu-profiler").unwrap(),
            ("MerlijnW70".to_string(), "cu-profiler".to_string())
        );
    }

    #[test]
    fn split_repo_rejects_malformed() {
        for bad in ["", "owner", "owner/", "/repo", "a/b/c"] {
            assert!(split_repo(bad).is_err(), "should reject `{bad}`");
        }
    }

    #[test]
    fn pr_from_event_reads_pull_request_number() {
        let v = json!({ "pull_request": { "number": 42 } });
        assert_eq!(pr_from_event_json(&v), Some(42));
    }

    #[test]
    fn pr_from_event_falls_back_to_top_level_and_issue() {
        assert_eq!(pr_from_event_json(&json!({ "number": 7 })), Some(7));
        assert_eq!(
            pr_from_event_json(&json!({ "issue": { "number": 9 } })),
            Some(9)
        );
        assert_eq!(pr_from_event_json(&json!({ "ref": "x" })), None);
    }

    #[test]
    fn pr_from_ref_parses_pull_refs() {
        assert_eq!(pr_from_ref("refs/pull/123/merge"), Some(123));
        assert_eq!(pr_from_ref("refs/heads/main"), None);
    }

    #[cfg(feature = "remote")]
    #[test]
    fn find_in_page_matches_the_marker() {
        let comments = vec![
            json!({ "id": 1, "body": "unrelated" }),
            json!({ "id": 2, "body": "<!-- cu-profiler-report -->\nhi" }),
        ];
        assert_eq!(find_in_page(&comments, "cu-profiler-report"), Some(2));
        assert_eq!(find_in_page(&comments, "other-marker"), None);
    }
}
