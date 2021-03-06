//! manage reading the verb shortcuts from the configuration file,
//! initializing if if it doesn't yet exist

use {
    crate::{
        errors::ConfError,
        keys,
        skin::SkinEntry,
        tree::*,
        verb::VerbConf,
    },
    crossterm::style::Attribute,
    directories::ProjectDirs,
    std::{
        collections::HashMap,
        fs, io,
        path::{Path, PathBuf},
        result::Result,
    },
    toml::{self, Value},
};

/// The configuration read from conf.toml file(s)
#[derive(Default)]
pub struct Conf {
    pub default_flags: String, // the flags to apply before cli ones
    pub date_time_format: Option<String>,
    pub verbs: Vec<VerbConf>,
    pub skin: HashMap<String, SkinEntry>,
    pub special_paths: Vec<SpecialPath>,
}

fn string_field(value: &Value, field_name: &str) -> Option<String> {
    if let Value::Table(tbl) = value {
        if let Some(fv) = tbl.get(field_name) {
            if let Some(s) = fv.as_str() {
                return Some(s.to_string());
            }
        }
    }
    None
}

fn bool_field(value: &Value, field_name: &str) -> Option<bool> {
    if let Value::Table(tbl) = value {
        if let Some(Value::Boolean(b)) = tbl.get(field_name) {
            return Some(*b);
        }
    }
    None
}

/// return the instance of ProjectDirs holding broot's specific paths
pub fn app_dirs() -> ProjectDirs {
    ProjectDirs::from("org", "dystroy", "broot").expect("Unable to find configuration directories")
}

/// return the path to the config directory, based on XDG
pub fn dir() -> PathBuf {
    app_dirs().config_dir().to_path_buf()
}

impl Conf {

    pub fn default_location() -> &'static Path {
        lazy_static! {
            static ref CONF_PATH: PathBuf = dir().join("conf.toml");
        }
        &*CONF_PATH
    }

    /// read the configuration file from the default OS specific location.
    /// Create it if it doesn't exist
    pub fn from_default_location() -> Result<Conf, ConfError> {
        let conf_filepath = Conf::default_location();
        if !conf_filepath.exists() {
            Conf::write_sample(&conf_filepath)?;
            println!(
                "New Configuration file written in {}{:?}{}.",
                Attribute::Bold,
                &conf_filepath,
                Attribute::Reset,
            );
            println!("You should have a look at it.");
        }
        let mut conf = Conf::default();
        match conf.read_file(&conf_filepath) {
            Err(e) => {
                println!("Failed to read configuration in {:?}.", &conf_filepath);
                println!("Please delete or fix this file.");
                Err(e)
            }
            _ => Ok(conf),
        }
    }

    /// assume the file doesn't yet exist
    pub fn write_sample(filepath: &Path) -> Result<(), io::Error> {
        fs::create_dir_all(filepath.parent().unwrap())?;
        fs::write(filepath, DEFAULT_CONF_FILE)?;
        Ok(())
    }

    /// read the configuration from a given path. Assume it exists.
    /// stderr is supposed to be a valid solution for displaying errors
    /// (i.e. this function is called before or after the terminal alternation)
    pub fn read_file(&mut self, filepath: &Path) -> Result<(), ConfError> {
        let data = fs::read_to_string(filepath)?;
        let root: Value = data.parse::<Value>()?;
        // reading default flags
        if let Some(s) = string_field(&root, "default_flags") {
            // it's additive because another config file may have
            // been read before and we usually want all the flags
            // (the last ones may reverse the first ones)
            self.default_flags.push_str(&s);
        }
        // date/time format
        self.date_time_format = string_field(&root, "date_time_format");
        // reading verbs
        if let Some(Value::Array(verbs_value)) = &root.get("verbs") {
            for verb_value in verbs_value.iter() {
                let invocation = string_field(verb_value, "invocation");
                let key = string_field(verb_value, "key")
                    .map(|s| keys::parse_key(&s))
                    .transpose()?;
                if let Some(key) = key {
                    if keys::is_reserved(key) {
                        return Err(ConfError::ReservedKey {
                            key: keys::key_event_desc(key),
                        });
                    }
                }
                let execution = match string_field(verb_value, "execution") {
                    Some(s) => s,
                    None => {
                        eprintln!("Invalid [[verbs]] entry in configuration");
                        eprintln!("Missing execution");
                        continue;
                    }
                };
                let from_shell = bool_field(verb_value, "from_shell");
                let leave_broot = bool_field(verb_value, "leave_broot");
                if leave_broot == Some(false) && from_shell == Some(true) {
                    eprintln!("Invalid [[verbs]] entry in configuration");
                    eprintln!(
                        "You can't simultaneously have leave_broot=false and from_shell=true"
                    );
                    continue;
                }
                let verb_conf = VerbConf {
                    invocation,
                    execution,
                    key,
                    shortcut: string_field(verb_value, "shortcut"),
                    description: string_field(verb_value, "description"),
                    from_shell,
                    leave_broot,
                };

                self.verbs.push(verb_conf);
            }
        }
        // reading the skin
        if let Some(Value::Table(entries_tbl)) = &root.get("skin") {
            for (k, v) in entries_tbl.iter() {
                if let Some(s) = v.as_str() {
                    match SkinEntry::parse(s) {
                        Ok(sec) => {
                            self.skin.insert(k.to_string(), sec);
                        }
                        Err(e) => {
                            eprintln!("{}", e);
                        }
                    }
                }
            }
        }

        // reading special paths
        if let Some(Value::Table(paths_tbl)) = &root.get("special-paths") {
            for (k, v) in paths_tbl.iter() {
                if let Some(v) = v.as_str() {
                    match SpecialPath::parse(k, v) {
                        Ok(sp) => {
                            debug!("Adding special path: {:?}", &sp);
                            self.special_paths.push(sp);
                        }
                        Err(e) => {
                            eprintln!("{}", e);
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

const DEFAULT_CONF_FILE: &str = r#"
###############################################################
# This configuration file lets you
# - define new commands
# - change the shortcut or triggering keys of built-in verbs
# - change the colors
# - set default values for flags
#
# Configuration documentation is available at
#     https://dystroy.org/broot
###############################################################

###############################################################
# Default flags
# You can set up flags you want broot to start with by
# default, for example `default_flags="ihp"` if you usually want
# to see hidden and gitignored files and the permissions (then
# if you don't want the hidden files you can launch `br -H`)
# A popular flag is the `g` one which displays git related info.
#
default_flags = ""

###############################################################
# Date/Time format
# If you want to change the format for date/time, uncomment the
# following line and change it according to
# https://docs.rs/chrono/0.4.11/chrono/format/strftime/index.html
#
# date_time_format = "%Y/%m/%d %R "

###############################################################
# Special paths
# If some paths must be handled specially, uncomment (and change
# this section as per the examples
#
# [special-paths]
# "/media/slow-backup-disk" = "no-enter"
# "/home/dys/useless" = "hide"
# "/home/dys/my-link-I-want-to-explore" = "enter"

###############################################################
# Verbs and shortcuts
# You can define your own commands which would be applied to
# the selection.
#
# Exemple 1: launching `tail -n` on the selected file (leaving broot)
# [[verbs]]
# name = "tail_lines"
# invocation = "tl {lines_count}"
# execution = "tail -f -n {lines_count} {file}"
#
# Exemple 2: creating a new file without leaving broot
# [[verbs]]
# name = "touch"
# invocation = "touch {new_file}"
# execution = "touch {directory}/{new_file}"
# leave_broot = false

# If $EDITOR isn't set on your computer, you should either set it using
#  something similar to
#   export EDITOR=/usr/bin/nvim
#  or just replace it with your editor of choice in the 'execution'
#  pattern.
# Example:
#  execution = "/usr/bin/nvim {file}"
[[verbs]]
invocation = "edit"
key = "F2"
shortcut = "e"
execution = "$EDITOR {file}"

[[verbs]]
invocation = "create {subpath}"
execution = "$EDITOR {directory}/{subpath}"

[[verbs]]
invocation = "git_diff"
shortcut = "gd"
leave_broot = false
execution = "git diff {file}"

# If $PAGER isn't set on your computer, you should either set it
#  or just replace it with your viewer of choice in the 'execution'
#  pattern.
# Example:
#  execution = "less {file}"
[[verbs]]
name = "view"
invocation = "view"
execution = "$PAGER {file}"

# A popular set of shorctuts for going up and down:
#
# [[verbs]]
# key = "ctrl-j"
# execution = ":line_down"
#
# [[verbs]]
# key = "ctrl-k"
# execution = ":line_up"
#
# [[verbs]]
# key = "ctrl-d"
# execution = ":page_down"
#
# [[verbs]]
# key = "ctrl-u"
# execution = ":page_up"

# If you develop using git, you might like to often switch
# to the "git status" filter:
# [[verbs]]
# key = "ctrl-g"
# execution = ":toggle_git_status"

# You can reproduce the bindings of Norton Commander
# on copying or moving to the other panel:
#
# [[verbs]]
# key = "F5"
# execution = ":copy_to_panel"
#
# [[verbs]]
# key = "F6"
# execution = ":move_to_panel"


###############################################################
# Skin
# If you want to change the colors of broot,
# uncomment the following bloc and start messing
# with the various values.
#
# [skin]
# default = "gray(23) none / gray(20) none"
# tree = "ansi(94) None / gray(3) None"
# file = "gray(18) None / gray(15) None"
# directory = "ansi(208) None Bold / ansi(172) None bold"
# exe = "Cyan None"
# link = "Magenta None"
# pruning = "gray(12) None Italic"
# perm__ = "gray(5) None"
# perm_r = "ansi(94) None"
# perm_w = "ansi(132) None"
# perm_x = "ansi(65) None"
# owner = "ansi(138) None"
# group = "ansi(131) None"
# dates = "ansi(66) None"
# sparse = "ansi(214) None"
# git_branch = "ansi(229) None"
# git_insertions = "ansi(28) None"
# git_deletions = "ansi(160) None"
# git_status_current = "gray(5) None"
# git_status_modified = "ansi(28) None"
# git_status_new = "ansi(94) None Bold"
# git_status_ignored = "gray(17) None"
# git_status_conflicted = "ansi(88) None"
# git_status_other = "ansi(88) None"
# selected_line = "None gray(5) / None gray(4)"
# char_match = "Yellow None"
# file_error = "Red None"
# flag_label = "gray(15) None"
# flag_value = "ansi(208) None Bold"
# input = "White None / gray(15) gray(2)"
# status_error = "gray(22) ansi(124)"
# status_job = "ansi(220) gray(5)"
# status_normal = "gray(20) gray(3) / gray(2) gray(2)"
# status_italic = "ansi(208) gray(3) / gray(2) gray(2)"
# status_bold = "ansi(208) gray(3) Bold / gray(2) gray(2)"
# status_code = "ansi(229) gray(3) / gray(2) gray(2)"
# status_ellipsis = "gray(19) gray(1) / gray(2) gray(2)"
# purpose_normal = "gray(20) gray(2)"
# purpose_italic = "ansi(178) gray(2)"
# purpose_bold = "ansi(178) gray(2) Bold"
# purpose_ellipsis = "gray(20) gray(2)"
# scrollbar_track = "gray(7) None / gray(4) None"
# scrollbar_thumb = "gray(22) None / gray(14) None"
# help_paragraph = "gray(20) None"
# help_bold = "ansi(208) None Bold"
# help_italic = "ansi(166) None"
# help_code = "gray(21) gray(3)"
# help_headers = "ansi(208) None"
# help_table_border = "ansi(239) None"

# You may find explanations and other skins on
#  https://dystroy.org/broot/skins
# for example a skin suitable for white backgrounds

"#;
