//! `daps __complete`: candidates for shell completion, from the local cache only.
//! No AWS, no network, no writes, and never anything on stderr: every error
//! means no output.

use crate::completer::{ParameterCompleter, complete_path};
use crate::encryption::Encryption;
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::Path;

/// Cache files in `dir` whose name starts with `prefix` and ends with `.txt`, sorted.
fn cache_files(dir: &Path, prefix: &str) -> Vec<String> {
    let mut files: Vec<String> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.starts_with(prefix) && n.ends_with(".txt"))
        .collect();
    files.sort();
    files
}

/// `key: rest` lines of a cache file; unreadable files and garbage lines are skipped.
fn read_pairs(path: &Path) -> Vec<(String, String)> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_once(": "))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect()
}

fn param_tree(dir: &Path) -> HashMap<String, Vec<String>> {
    let mut tree = HashMap::new();
    for f in cache_files(dir, "parameters_") {
        for (path, _) in read_pairs(&dir.join(f)) {
            ParameterCompleter::process_parameter_path(&path, &mut tree);
        }
    }
    tree
}

fn params(dir: &Path, word: &str) -> Vec<String> {
    let tree = param_tree(dir);
    let mut out: Vec<String> = complete_path(&tree, word)
        .into_iter()
        .map(|c| {
            if tree.get(&c).is_some_and(|kids| !kids.is_empty()) {
                format!("{}/", c)
            } else {
                c
            }
        })
        .collect();
    out.sort();
    out
}

fn secrets(dir: &Path, word: &str) -> Vec<String> {
    let word = word.to_lowercase();
    let mut out: Vec<String> = read_pairs(&dir.join("secrets__.txt"))
        .into_iter()
        .map(|(name, _)| name)
        .filter(|n| n.to_lowercase().starts_with(&word))
        .collect();
    out.sort();
    out.dedup();
    out
}

/// The one decrypted value for exactly `key`, or nothing. Multi-line values and
/// decrypt failures give nothing.
fn value(dir: &Path, key: &str, secret: bool, enc_key: &str) -> Vec<String> {
    let files = if secret {
        vec!["secrets__.txt".to_string()]
    } else {
        cache_files(dir, "values_")
    };
    let enc = Encryption::new(true, enc_key.to_string());
    for f in files {
        if let Some((_, v)) = read_pairs(&dir.join(f)).into_iter().find(|(k, _)| k == key) {
            let plain = enc.decrypt_value(&v);
            if plain.is_empty() || plain.contains(['\n', '\r']) || plain.starts_with("decryption error:") {
                return Vec::new();
            }
            return vec![plain];
        }
    }
    Vec::new()
}

/// Candidates for `kind` (param | secret | value | secret-value).
/// Values print nothing unless `values_enabled`.
pub fn candidates(kind: &str, word: &str, dir: &Path, values_enabled: bool, enc_key: &str) -> Vec<String> {
    match kind {
        "param" => params(dir, word),
        "secret" => secrets(dir, word),
        "value" if values_enabled => value(dir, word, false, enc_key),
        "secret-value" if values_enabled => value(dir, word, true, enc_key),
        _ => Vec::new(),
    }
}

/// Values complete unless `DAPS_COMPLETE_VALUES` is exactly `0`.
fn values_enabled(var: Option<&str>) -> bool {
    var != Some("0")
}

pub fn run(kind: &str, word: &str, store_dir: &str, enc_key: &str) {
    let enabled = values_enabled(std::env::var("DAPS_COMPLETE_VALUES").ok().as_deref());
    let out = candidates(kind, word, Path::new(store_dir), enabled, enc_key);
    let mut stdout = std::io::stdout().lock();
    for line in out {
        // A closed pipe must not turn into a panic message on the terminal.
        if writeln!(stdout, "{}", line).is_err() {
            break;
        }
    }
}

/// (generated spec text, replacement action, times it must occur). Anything
/// left over becomes no completion, never local filenames.
/// Also: clap's zsh spec gives value-taking options no argument, so zsh would take
/// `--store-dir X` as a positional and never reach the subcommand. Give them one.
const ZSH_ACTIONS: &[(&str, &str, usize)] = &[
    (":path -- Parameter path (e.g. /prod/db/password):_files'", ":path -- Parameter path (e.g. /prod/db/password):_daps_params'", 1),
    (":path -- Path prefix (e.g. /prod/):_files'", ":path -- Path prefix (e.g. /prod/):_daps_params'", 1),
    // set, insert, reload
    (":path -- Parameter path:_files'", ":path -- Parameter path:_daps_params'", 3),
    (":paths -- Path prefixes to reload (e.g. /prod/ /staging/):_files'", ":paths -- Path prefixes to reload (e.g. /prod/ /staging/):_daps_params'", 1),
    (":path -- Parameter path whose value is a connection string:_files'", ":path -- Parameter path whose value is a connection string:_daps_params'", 1),
    // secret-get, secret-set
    (":name -- Secret name or ARN:_files'", ":name -- Secret name or ARN:_daps_secrets'", 2),
    (":value -- New value (reads from stdin if omitted and stdin is a pipe):_files'", ":value -- New value (reads from stdin if omitted and stdin is a pipe):_daps_value'", 1),
    (":value -- New secret value (reads from stdin if omitted and stdin is a pipe):_files'", ":value -- New secret value (reads from stdin if omitted and stdin is a pipe):_daps_value secret'", 1),
    ("--region=[AWS Region]'", "--region=[AWS Region]:region: '", 1),
    ("-p+[Starting path for parameter store (e.g., /prod/)]'", "-p+[Starting path for parameter store (e.g., /prod/)]:path: '", 1),
    ("--path=[Starting path for parameter store (e.g., /prod/)]'", "--path=[Starting path for parameter store (e.g., /prod/)]:path: '", 1),
    ("--store-dir=[Store directory for parameters and values]'", "--store-dir=[Store directory for parameters and values]:dir:_directories'", 1),
    ("-t+[Parameter type: String | StringList | SecureString]'", "-t+[Parameter type: String | StringList | SecureString]:type: '", 1),
    ("--type=[Parameter type: String | StringList | SecureString]'", "--type=[Parameter type: String | StringList | SecureString]:type: '", 1),
    ("-d+[Optional description]'", "-d+[Optional description]:description: '", 1),
    ("--description=[Optional description]'", "--description=[Optional description]:description: '", 1),
    ("--if-changed=[Print one line instead of the guide if its hash equals HASH]'", "--if-changed=[Print one line instead of the guide if its hash equals HASH]:hash: '", 1),
    (":_files'", ": '", 5),
];

const ZSH_FUNCTIONS: &str = r#"
# Candidates from the local cache: _daps_out KIND WORD. clap resets $words on
# entering a subcommand, so read the whole line for --store-dir.
_daps_out() {
    local sd=parameters i
    local -a t; t=(${(Q)${(z)BUFFER}})
    for (( i = 1; i < $#t; i++ )); do
        case $t[i] in
            --store-dir) sd=$t[i+1] ;;
            --store-dir=*) sd=${t[i]#--store-dir=} ;;
        esac
    done
    command daps __complete $1 "$2" $sd 2>/dev/null
}
_daps_params() {
    local -a c; c=(${(f)"$(_daps_out param "$PREFIX")"})
    (( $#c )) && compadd -q -S '' -- $c
}
_daps_secrets() {
    local -a c; c=(${(f)"$(_daps_out secret "$PREFIX")"})
    (( $#c )) && compadd -q -S '' -- $c
}
# Current value of the previous word (set PATH <TAB>); DAPS_COMPLETE_VALUES=0 turns it off.
_daps_value() {
    local k=value v
    [[ $1 == secret ]] && k=secret-value
    v=$(_daps_out $k "$words[CURRENT-1]")
    [[ -n $v ]] && compadd -- "$v"
}
"#;

/// Clap's zsh script with `_files` replaced by cache-backed completion.
pub fn zsh_script(raw: &str) -> String {
    let mut s = raw.to_string();
    for (from, to, _) in ZSH_ACTIONS {
        s = s.replace(from, to);
    }
    // Keep the trailing `_daps "$@"` call last so the functions exist when it runs.
    match s.rfind("\n_daps \"$@\"") {
        Some(i) => s.insert_str(i, ZSH_FUNCTIONS),
        None => s.push_str(ZSH_FUNCTIONS),
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zsh_replacements_all_apply() {
        use structopt::StructOpt;
        let mut raw = Vec::new();
        crate::Opt::clap().gen_completions_to("daps", structopt::clap::Shell::Zsh, &mut raw);
        let raw = String::from_utf8(raw).unwrap();
        for (from, _, n) in ZSH_ACTIONS {
            // `: '`-style catch-all counts what is left after the specific ones.
            if *from != ":_files'" {
                assert_eq!(raw.matches(from).count(), *n, "{}", from);
            }
        }
        let out = zsh_script(&raw);
        assert!(!out.contains("_files"));
        assert!(out.contains("_daps_params") && out.contains("_daps_secrets") && out.contains("_daps_value"));
        assert!(out.trim_end().ends_with("_daps \"$@\""));
    }

    const KEY: &str = "test-key";

    fn store() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("daps-complete-{}-{:?}", std::process::id(), std::thread::current().id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let e = Encryption::new(true, KEY.to_string());
        fs::write(dir.join("parameters__flex.txt"), "/flex/uat/svc/a: []\n/flex/uat/b: []\nnot a pair\n").unwrap();
        fs::write(dir.join("parameters__.txt"), "/other/x: []\n").unwrap();
        fs::write(
            dir.join("values__flex.txt"),
            format!("/flex/uat/b: {}\n/flex/uat/multi: {}\n", e.encrypt_value("fake-one"), e.encrypt_value("l1\nl2")),
        )
        .unwrap();
        fs::write(
            dir.join("secrets__.txt"),
            format!("Prod/Db: {}\nprod/api: {}\nzzz: {}\n", e.encrypt_value("fake-s1"), e.encrypt_value("x"), e.encrypt_value("y")),
        )
        .unwrap();
        dir
    }

    #[test]
    fn params_prefix_and_slash_on_branches() {
        let d = store();
        assert_eq!(candidates("param", "/fl", &d, false, KEY), ["/flex/"]);
        assert_eq!(candidates("param", "/flex/uat/", &d, false, KEY), ["/flex/uat/b", "/flex/uat/svc/"]);
        assert_eq!(candidates("param", "/o", &d, false, KEY), ["/other/"]);
        assert!(candidates("param", "/nope/", &d, false, KEY).is_empty());
    }

    #[test]
    fn secrets_prefix_case_insensitive_names_only() {
        let d = store();
        assert_eq!(candidates("secret", "PROD/", &d, false, KEY), ["Prod/Db", "prod/api"]);
        assert!(candidates("secret", "q", &d, false, KEY).is_empty());
    }

    #[test]
    fn values_env_default_on() {
        assert!(values_enabled(None));
        assert!(values_enabled(Some("1")));
        assert!(values_enabled(Some("")));
        assert!(!values_enabled(Some("0")));
    }

    #[test]
    fn values_off_when_disabled() {
        let d = store();
        assert!(candidates("value", "/flex/uat/b", &d, false, KEY).is_empty());
        assert!(candidates("secret-value", "Prod/Db", &d, false, KEY).is_empty());
        assert_eq!(candidates("value", "/flex/uat/b", &d, true, KEY), ["fake-one"]);
        assert_eq!(candidates("secret-value", "Prod/Db", &d, true, KEY), ["fake-s1"]);
    }

    #[test]
    fn values_fail_closed() {
        let d = store();
        assert!(candidates("value", "/flex/uat/missing", &d, true, KEY).is_empty());
        assert!(candidates("value", "/flex/uat/multi", &d, true, KEY).is_empty());
        assert!(candidates("value", "/flex/uat/b", &d, true, "wrong-key").is_empty());
    }

    #[test]
    fn missing_store_dir_is_empty_for_every_kind() {
        let d = Path::new("/nonexistent/daps-store");
        for k in ["param", "secret", "value", "secret-value", "bogus"] {
            assert!(candidates(k, "/x", d, true, KEY).is_empty());
        }
    }
}
