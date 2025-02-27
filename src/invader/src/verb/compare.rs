use std::collections::{HashMap, VecDeque};
use std::env::Args;
use std::path::Path;
use std::sync::{Arc, Mutex};
use crate::cli::{CommandLineParser, CommandLineValue, CommandLineValueType, Parameter};
use ringhopper::error::Error;
use ringhopper::map::load_map_from_filesystem_as_tag_tree;
use ringhopper::tag::default::set_all_defaults_for_tag;
use ringhopper::primitives::primitive::TagPath;
use ringhopper::primitives::tag::ParseStrictness;
use ringhopper::tag::compare::{compare_tags, TagComparisonDifference};
use ringhopper::tag::tree::{TagTree, VirtualTagsDirectory};
use crate::threading::{DisplayMode, do_with_threads, ProcessSuccessType};
use crate::util::make_stdout_logger;

use crate::util::StdoutLogger;

#[derive(PartialEq)]
enum Show {
    All,
    Mismatched,
    Matched
}

#[derive(Clone)]
struct UserData {
    tags: Arc<dyn TagTree + Send + Sync>,
    differences: Arc<Mutex<HashMap<TagPath, Vec<TagComparisonDifference>>>>,
    raw: bool,
    abbreviated: bool
}

pub fn compare(args: Args, description: &'static str) -> Result<(), String> {
    let parser = CommandLineParser::new(description, "<source1> <source2> [args]")
        .add_help()
        .add_custom_parameter(Parameter::new(
            "verbose",
            'v',
            "Display detailed output.",
            "",
            None,
            0,
            None,
            false,
            false
        ))
        .add_custom_parameter(Parameter::new(
            "show",
            's',
            "Set whether to display `all`, `matched`, or `mismatched`. Default: `mismatched`",
            "<param>",
            Some(CommandLineValueType::String),
            1,
            Some(vec![CommandLineValue::String("mismatched".to_owned())]),
            false,
            false
        ))
        .add_custom_parameter(Parameter::new(
            "filter",
            'f',
            "Filter tags to be compared. By default, all tags are compared.",
            "<tag.group*>",
            Some(CommandLineValueType::String),
            1,
            Some(vec![CommandLineValue::String("*".to_owned())]),
            false,
            false
        ))
        .add_custom_parameter(Parameter::new(
            "abbreviated",
            'a',
            "Minimize reflexives that have multiple differences.",
            "",
            None,
            0,
            None,
            false,
            false
        ))
        .add_jobs()
        .add_custom_parameter(Parameter::single("raw", 'r', "Also compare cache-only fields, and disable defaulting when comparing.", "", None))
        .set_required_extra_parameters(2)
        .parse(args)?;

    let display_mode = match parser.get_custom("show").unwrap()[0].string() {
        "all" => Show::All,
        "mismatched" => Show::Mismatched,
        "matched" => Show::Matched,
        n => return Err(format!("Invalid --show parameter {n}"))
    };

    let verbose = parser.get_custom("verbose").is_some();
    let raw = parser.get_custom("raw").is_some();
    let abbreviated = parser.get_custom("abbreviated").is_some();

    let mut source: VecDeque<Arc<dyn TagTree + Send + Sync>> = VecDeque::new();
    for i in parser.get_extra() {
        let path: &Path = i.as_ref();
        if !path.exists() {
            return Err(format!("Source `{i}` does not exist"))
        }
        else if path.is_file() && path.extension() == Some("map".as_ref()) {
            let map = load_map_from_filesystem_as_tag_tree(path, ParseStrictness::Strict)
                .map_err(|e| format!("Cannot load {path:?} as a cache file: {e}"))?;
            source.push_back(map);
        }
        else if path.is_dir() {
            let dir = match VirtualTagsDirectory::new(&[path], None) {
                Ok(n) => n,
                Err(e) => return Err(format!("Error with tags directory {}: {e}", path.display()))
            };
            source.push_back(Arc::new(dir));
        }
        else {
            return Err(format!("Source `{i}` does not refer to a directory"))
        }
    }

    let primary = source.pop_front().unwrap();
    let secondary = source.pop_front().unwrap();
    let tag = parser.get_custom("filter").unwrap()[0].string().to_owned();

    let user_data = UserData {
        tags: secondary,
        differences: Arc::new(Mutex::new(HashMap::new())),
        raw,
        abbreviated
    };

    let logger = make_stdout_logger();

    do_with_threads(primary, parser, &tag, None, user_data.clone(), DisplayMode::Silent, logger.clone(), |context, path, user_data, _| {
        let mut secondary = match user_data.tags.open_tag_copy(path) {
            Ok(n) => n,
            Err(Error::TagNotFound(_)) => return Ok(ProcessSuccessType::Skipped("not in directory")),
            n => n?
        };

        let mut primary = context.tags_directory.open_tag_copy(path)?;

        // Set defaults (functional comparison)
        if !user_data.raw {
            set_all_defaults_for_tag(primary.as_mut());
            set_all_defaults_for_tag(secondary.as_mut());
        }

        let differences = compare_tags(primary.as_ref(), secondary.as_ref(), user_data.raw, user_data.abbreviated);
        user_data.differences.lock().unwrap().insert(path.to_owned(), differences);

        Ok(ProcessSuccessType::Success)
    })?;

    display_result(display_mode, verbose, user_data, &logger);

    Ok(())
}

fn display_result(display_mode: Show, verbose: bool, user_data: UserData, logger: &Arc<StdoutLogger>) {
    let mut matched = 0usize;
    let all_differences = Arc::into_inner(user_data.differences).unwrap().into_inner().unwrap();
    let mut keys: Vec<TagPath> = all_differences.keys().map(|c| c.to_owned()).collect();
    keys.sort();

    for k in &keys {
        let differences = &all_differences[k];
        if !differences.is_empty() {
            if display_mode == Show::All || display_mode == Show::Mismatched {
                logger.warning_fmt_ln(format_args!("Mismatched: {k}"));
                if verbose {
                    display_diff(differences, &logger);
                }
            }
        }
        else {
            if display_mode == Show::All || display_mode == Show::Matched {
                logger.success_fmt_ln(format_args!("Matched: {k}"));
            }
            matched += 1;
        }
    }

    logger.neutral_fmt_ln(format_args!("Matched {matched} / {} tag{s}", all_differences.len(), s = if all_differences.len() == 1 { "" } else { "s" }));
}

#[derive(Default)]
struct DifferenceMap {
    field: String,
    difference: Option<TagComparisonDifference>,
    children: Vec<DifferenceMap>
}

impl DifferenceMap {
    fn access_mut(&mut self, path: &str) -> &mut DifferenceMap {
        match path.find('.') {
            Some(n) => {
                let (field, remaining) = path.split_at(n);
                self.access_mut(field)
                    .access_mut(&remaining[1..])
            },
            None => {
                self.create_if_not_exists(path)
            }
        }
    }

    fn create_if_not_exists(&mut self, field: &str) -> &mut DifferenceMap {
        let index = self.get(field);
        &mut self.children[index]
    }

    fn get(&mut self, field: &str) -> usize {
        let len = self.children.len();
        for i in 0..self.children.len() {
            if self.children[i].field == field {
                return i
            }
        }
        self.children.push(DifferenceMap {
            field: field.to_owned(),
            difference: None,
            children: Vec::new()
        });
        len
    }
}

fn display_item(what: &DifferenceMap, depth: usize, io: &Arc<StdoutLogger>) {
    if depth > 0 {
        for _ in 0..depth {
            io.neutral("    ");
        }
        io.warning(&what.field);
        if let Some(n) = &what.difference {
            io.warning_fmt(format_args!(": {}", n.difference));
        }
        io.neutral_ln("");
    }
    for i in &what.children {
        display_item(i, depth + 1, io);
    }
}

fn display_diff(diff: &Vec<TagComparisonDifference>, io: &Arc<StdoutLogger>) {
    let mut map = DifferenceMap::default();
    for i in diff {
        map.access_mut(&i.path).difference = Some(i.clone());
    }
    display_item(&map, 0, io);
}
