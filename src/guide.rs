//! `daps guide`: a self-contained reference for agents and humans.
//! Runs before any AWS setup: no credentials, no network.

use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

const GUIDE: &str = "\
daps - command-line tool for AWS Systems Manager Parameter Store (and Secrets Manager)

MODES
  daps                REPL (interactive, tab completion). Not for agents.
  daps <subcommand>   one-shot, scriptable, exits when done. Agents: use this.

GLOBAL FLAGS (before the subcommand)
  -p, --path P        base path, must start with / (default /)
  --region R          AWS region (default us-east-1)
  -r, --refresh       refresh the local cache on startup
  --store-dir D       cache directory (default ~/parameters)
  --verbose           extra diagnostics on stderr
  AWS credentials come from the standard AWS environment/config chain.

READ (no side effects on AWS)
  get PATH            one parameter; prints the raw value only
  gets PATH           all parameters under a prefix; key<TAB>value lines
  gets PATH --keys-only   keys only, one per line
  search TERM         fuzzy-search cached keys; key<TAB>value lines
  search TERM --keys-only   keys only
  parse-db PATH       parameter holds a connection string (k=v;k=v);
                      piped output is KEY=value lines
  secret-get NAME     one secret's value (Secrets Manager)
  secret-list [FILTER]   list secret names, optional substring filter

CACHE (local files; reload/refresh read AWS)
  reload PATH         re-fetch one parameter into the cache
  reload-paths P...   re-fetch everything under one or more prefixes
  refresh             re-fetch the whole tree under --path
  migrate             re-encrypt the cache with the current key

WRITE - these change AWS. Confirm with the user first.
  set PATH [VALUE]    update an existing parameter
  insert PATH VALUE [-t TYPE]   create one; TYPE is String (default),
                      StringList or SecureString
  secret-set NAME [VALUE]       update an existing secret
  secret-create NAME VALUE [-d DESCRIPTION]   create a secret
  set and secret-set read the value from stdin when it is piped, so it
  stays out of shell history:  printf %s \"$V\" | daps set /app/key

LOCAL CACHE
  Lives in --store-dir and is encrypted with the DAPS_ENCRYPTION_KEY
  environment variable. search reads only the cache, so run refresh (or
  -r) first if it is empty or stale. Without the variable set, a built-in
  default key is used: set your own key.

SAFETY AND COST
  - Values are secrets. Do not echo them into logs or chat. When you only
    need names, use --keys-only.
  - Scope paths narrowly. A broad `gets /` or `refresh` walks every
    parameter and can be slow and costly.
  - Errors go to stderr with a non-zero exit code.

SHELL COMPLETION
  daps completion zsh > ~/.zsh/_daps   completes paths and secret names from
  the local cache, and current cached values for set/secret-set (the
  decrypted value shows on screen and in history if submitted).
  DAPS_COMPLETE_VALUES=0 turns value completion off.

RE-CHECKING THIS GUIDE
  daps guide --if-changed HASH   one line if the guide is unchanged
  daps guide --if-new            for humans: remembers what you last read
";

fn body_hash(body: &str) -> String {
    let digest = Sha256::digest(body.as_bytes());
    digest.iter().take(6).map(|b| format!("{:02x}", b)).collect()
}

fn full_guide(body: &str) -> String {
    format!(
        "guide-hash: {}  (to re-check later: daps guide --if-changed {})\n\n{}",
        body_hash(body),
        body_hash(body),
        body
    )
}

fn if_changed(body: &str, given: &str) -> String {
    let h = body_hash(body);
    if given == h {
        format!(
            "guide unchanged (guide-hash {}); the copy you already read is current.\n",
            h
        )
    } else {
        full_guide(body)
    }
}

fn plural(n: u64, unit: &str) -> String {
    format!("{} {}{}", n, unit, if n == 1 { "" } else { "s" })
}

fn age(secs: u64) -> String {
    match secs {
        0..=59 => "just now".to_string(),
        60..=3599 => plural(secs / 60, "minute"),
        3600..=86399 => plural(secs / 3600, "hour"),
        _ => plural(secs / 86400, "day"),
    }
}

/// "<age> ago", or "just now" without the suffix.
fn ago(secs: u64) -> String {
    match age(secs) {
        a if a == "just now" => a,
        a => format!("{} ago", a),
    }
}

/// Parse `<hex> <unix_seconds>`. Anything else, or a time after `now`, is None.
fn parse_record(s: &str, now: u64) -> Option<(String, u64)> {
    let mut it = s.trim_end().split(' ');
    let (hash, ts) = (it.next()?, it.next()?);
    if it.next().is_some() || hash.is_empty() || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let ts: u64 = ts.parse().ok()?;
    if ts > now {
        return None;
    }
    Some((hash.to_string(), ts))
}

fn write_record(dir: &Path, hash: &str, now: u64) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        fs::DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    }
    #[cfg(not(unix))]
    fs::create_dir_all(dir)?;

    let tmp = dir.join(format!("guide-seen.tmp{}", std::process::id()));
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let res = opts
        .open(&tmp)
        .and_then(|mut f| f.write_all(format!("{} {}\n", hash, now).as_bytes()))
        .and_then(|_| fs::rename(&tmp, dir.join("guide-seen")));
    if res.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    res
}

/// Returns (stdout text, optional warning for stderr).
fn if_new(body: &str, dir: &Path, now: u64) -> (String, Option<String>) {
    let h = body_hash(body);
    let old = fs::read_to_string(dir.join("guide-seen"))
        .ok()
        .and_then(|s| parse_record(&s, now));

    let mut out = String::new();
    match old {
        Some((oh, ts)) if oh == h => {
            return (
                format!(
                    "guide unchanged since you read it {} · guide-hash {} · run without --if-new to read it again\n",
                    ago(now - ts),
                    h
                ),
                None,
            );
        }
        Some((oh, ts)) => out.push_str(&format!(
            "guide changed since you last read it {} (was {})\n",
            ago(now - ts),
            oh
        )),
        None => {}
    }
    out.push_str(&full_guide(body));
    let warn = write_record(dir, &h, now)
        .err()
        .map(|e| format!("warning: could not save guide-seen record: {}", e));
    (out, warn)
}

fn cache_dir() -> Option<PathBuf> {
    let var = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty());
    let base = match var("XDG_CACHE_HOME") {
        Some(x) => PathBuf::from(x),
        None => PathBuf::from(var("HOME")?).join(".cache"),
    };
    Some(base.join("daps"))
}

/// Entry point for `daps guide`.
pub fn run(changed: Option<String>, new_only: bool) {
    if let Some(given) = changed {
        print!("{}", if_changed(GUIDE, &given));
        return;
    }
    if !new_only {
        print!("{}", full_guide(GUIDE));
        return;
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    match cache_dir() {
        Some(dir) => {
            let (out, warn) = if_new(GUIDE, &dir, now);
            print!("{}", out);
            if let Some(w) = warn {
                eprintln!("{}", w);
            }
        }
        None => {
            print!("{}", full_guide(GUIDE));
            eprintln!("warning: no cache directory (set HOME or XDG_CACHE_HOME)");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    const B: &str = "body text\n";

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("daps-guide-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn guide_is_small() {
        assert!(full_guide(GUIDE).len() <= 4096);
    }

    #[test]
    fn hash_is_12_hex_and_stable() {
        let h = body_hash(B);
        assert_eq!(h.len(), 12);
        assert_eq!(h, body_hash(B));
        assert!(full_guide(B).starts_with(&format!("guide-hash: {}  (", h)));
    }

    #[test]
    fn if_changed_exact_match_only() {
        let h = body_hash(B);
        assert!(if_changed(B, &h).starts_with("guide unchanged (guide-hash"));
        assert_eq!(if_changed(B, "x"), full_guide(B));
        assert_eq!(if_changed(B, ""), full_guide(B));
    }

    #[test]
    fn age_wording() {
        assert_eq!(age(59), "just now");
        assert_eq!(age(60), "1 minute");
        assert_eq!(age(120), "2 minutes");
        assert_eq!(age(3600), "1 hour");
        assert_eq!(age(7200), "2 hours");
        assert_eq!(age(86400), "1 day");
        assert_eq!(age(3 * 86400), "3 days");
    }

    #[test]
    fn record_parsing_fails_closed() {
        assert!(parse_record("abc123 100\n", 200).is_some());
        assert!(parse_record("garbage", 200).is_none());
        assert!(parse_record("abc123 100 extra", 200).is_none());
        assert!(parse_record("xyz 100", 200).is_none());
        assert!(parse_record("abc123 -1", 200).is_none());
        assert!(parse_record("abc123 300", 200).is_none()); // future
        assert!(parse_record("", 200).is_none());
    }

    #[test]
    fn if_new_flow_and_perms() {
        let d = tmp("flow").join("daps");
        let (o1, w1) = if_new(B, &d, 1000);
        assert_eq!(o1, full_guide(B));
        assert!(w1.is_none());
        let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&d), 0o700);
        assert_eq!(mode(&d.join("guide-seen")), 0o600);

        let (o2, _) = if_new(B, &d, 1000 + 2 * 3600);
        assert!(o2.starts_with("guide unchanged since you read it 2 hours ago"));

        // different hash: "changed" line, full guide, record rewritten
        let (o3, _) = if_new("other\n", &d, 1000 + 86400);
        assert!(o3.starts_with("guide changed since you last read it 1 day ago (was "));
        assert!(o3.contains("guide-hash: "));
        let (o4, _) = if_new("other\n", &d, 1000 + 86400);
        assert!(o4.starts_with("guide unchanged"));
        let _ = fs::remove_dir_all(d.parent().unwrap());
    }

    #[test]
    fn if_new_corrupt_or_future_prints_full_and_rewrites() {
        let d = tmp("bad").join("daps");
        fs::create_dir_all(&d).unwrap();
        for bad in ["garbage\n", &format!("{} 9999\n", body_hash(B))] {
            fs::write(d.join("guide-seen"), bad).unwrap();
            let (o, _) = if_new(B, &d, 1000);
            assert_eq!(o, full_guide(B));
            let rec = fs::read_to_string(d.join("guide-seen")).unwrap();
            assert_eq!(rec, format!("{} 1000\n", body_hash(B)));
        }
        let _ = fs::remove_dir_all(d.parent().unwrap());
    }

    #[test]
    fn if_new_write_failure_warns_but_prints() {
        let d = tmp("fail");
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("daps"), "a file, not a dir").unwrap();
        let (o, w) = if_new(B, &d.join("daps"), 1000);
        assert_eq!(o, full_guide(B));
        assert!(w.unwrap().starts_with("warning:"));
        let _ = fs::remove_dir_all(&d);
    }
}
