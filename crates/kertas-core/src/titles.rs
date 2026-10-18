//! Telling apart open files with the same name (#95): each one's title gets
//! enough of its folder to tell it from the others, as VS Code and Xcode
//! do, e.g. `README.md — docs` and `README.md — src`.

use std::ffi::OsStr;
use std::path::{Component, Path};

/// What separates a file's name from its folder in a title.
const SEPARATOR: &str = " — ";

/// The folder hint for each of `paths`, the files open across every
/// window: `None` for a file whose name no other open file shares, and for
/// documents with no file.
///
/// A hint is the nearest folder that differs from the other files' with
/// the same name. When only a folder further up differs, the hint names it:
/// `a/x` if it's one folder up, otherwise `a/…`.
pub fn folder_hints(paths: &[Option<&Path>]) -> Vec<Option<String>> {
    paths
        .iter()
        .map(|path| {
            let path = (*path)?;
            let name = path.file_name()?;
            let others: Vec<Vec<&OsStr>> = paths
                .iter()
                .flatten()
                .filter(|other| other.file_name() == Some(name) && **other != path)
                .map(|other| folders(other))
                .collect();
            if others.is_empty() {
                return None;
            }
            let folders = folders(path);
            // How many folders, nearest first, it takes to differ from
            // every other file with the same name.
            let needed = (1..=folders.len()).find(|&count| {
                others
                    .iter()
                    .all(|other| other.get(..count) != Some(&folders[..count]))
            });
            Some(match needed {
                Some(1) => folders[0].to_string_lossy().into_owned(),
                Some(2) => format!(
                    "{}/{}",
                    folders[1].to_string_lossy(),
                    folders[0].to_string_lossy()
                ),
                Some(count) => format!("{}/…", folders[count - 1].to_string_lossy()),
                // Its folders all match the end of another file's, e.g.
                // `/a/README.md` and `/b/a/README.md`: show the whole folder.
                None => path
                    .parent()
                    .map_or_else(String::new, |folder| folder.display().to_string()),
            })
        })
        .collect()
}

/// A window or tab title: the file's name, with its folder hint if it has
/// one. The name comes first, so it survives the end being cut off.
pub fn title(name: &str, hint: Option<&str>) -> String {
    match hint {
        Some(hint) => format!("{name}{SEPARATOR}{hint}"),
        None => name.to_owned(),
    }
}

/// The names of a file's folders, nearest first.
fn folders(path: &Path) -> Vec<&OsStr> {
    let mut folders: Vec<&OsStr> = path
        .parent()
        .into_iter()
        .flat_map(Path::components)
        .filter_map(|component| match component {
            Component::Normal(name) => Some(name),
            _ => None,
        })
        .collect();
    folders.reverse();
    folders
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hints(paths: &[Option<&str>]) -> Vec<Option<String>> {
        let paths: Vec<Option<&Path>> = paths.iter().map(|path| path.map(Path::new)).collect();
        folder_hints(&paths)
    }

    fn some(hint: &str) -> Option<String> {
        Some(hint.to_owned())
    }

    #[test]
    fn unique_names_have_no_hint() {
        assert_eq!(
            hints(&[Some("/p/docs/README.md"), Some("/p/notes.md"), None]),
            [None, None, None]
        );
    }

    #[test]
    fn clashing_names_get_their_folder() {
        assert_eq!(
            hints(&[
                Some("/p/docs/README.md"),
                Some("/p/src/README.md"),
                Some("/p/notes.md"),
            ]),
            [some("docs"), some("src"), None]
        );
    }

    #[test]
    fn matching_folders_go_up_until_they_differ() {
        assert_eq!(
            hints(&[Some("/p/a/x/README.md"), Some("/p/b/x/README.md")]),
            [some("a/x"), some("b/x")]
        );
        assert_eq!(
            hints(&[Some("/p/a/x/y/README.md"), Some("/p/b/x/y/README.md")]),
            [some("a/…"), some("b/…")]
        );
    }

    #[test]
    fn each_file_differs_from_all_the_others() {
        assert_eq!(
            hints(&[
                Some("/p/a/x/README.md"),
                Some("/p/b/x/README.md"),
                Some("/p/c/README.md"),
            ]),
            [some("a/x"), some("b/x"), some("c")]
        );
    }

    #[test]
    fn a_folder_inside_the_other_shows_the_whole_folder() {
        assert_eq!(
            hints(&[Some("/a/README.md"), Some("/b/a/README.md")]),
            [some("/a"), some("b/a")]
        );
    }

    #[test]
    fn untitled_documents_are_left_alone() {
        assert_eq!(
            hints(&[None, None, Some("/p/Untitled")]),
            [None, None, None]
        );
    }

    #[test]
    fn titles_put_the_name_first() {
        assert_eq!(title("README.md", Some("docs")), "README.md — docs");
        assert_eq!(title("README.md", None), "README.md");
    }
}
