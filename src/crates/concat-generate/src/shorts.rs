// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors

//! A Short: a minute of a long video, cut again for a phone held upright.
//!
//! The scenes are the long video's own, run of them in a row, with the
//! narration and the shots already made for them: a Short costs nothing to
//! make but, when the writer picks the minute, a few hundred words. It is
//! laid on a timeline of its own, 9:16, its pictures filling the tall
//! frame and its captions a few words at a time.

use crate::storyboard::{Chapter, Storyboard};

/// The longest a Short runs, in seconds: what the platforms take.
pub const LONGEST: f64 = 60.0;

/// The minute picked: which scenes, in order, and the line over its start.
#[derive(Clone, Debug, PartialEq)]
pub struct Pick {
    /// The scenes' ids, a run of them in a row.
    pub scenes: Vec<String>,
    /// A few words over the first second, to stop the thumb.
    pub hook: String,
}

/// Every scene, in order, with its written length.
fn scenes(board: &Storyboard) -> Vec<(String, f64)> {
    board
        .scenes()
        .map(|scene| (scene.id.clone(), scene.seconds.max(0.0)))
        .collect()
}

/// Whether `ids` is a run of scenes in a row of `board` that fits in a
/// Short: what a pick from the writer is held to before it is used.
pub fn fits(board: &Storyboard, ids: &[String]) -> bool {
    let all = scenes(board);
    let Some(first) = ids
        .first()
        .and_then(|id| all.iter().position(|(scene, _)| scene == id))
    else {
        return false;
    };
    let run = all.get(first..first + ids.len());
    run.is_some_and(|run| {
        run.iter().zip(ids).all(|((scene, _), id)| scene == id)
            && run.iter().map(|(_, seconds)| seconds).sum::<f64>() <= LONGEST + 5.0
    })
}

/// The minute picked without the writer: the video's opening, as many
/// scenes from the first as fit in [`LONGEST`] - the part made to hold a
/// viewer - under the video's title.
pub fn opening(board: &Storyboard) -> Pick {
    let mut total = 0.0;
    let mut ids = Vec::new();
    for (id, seconds) in scenes(board) {
        if !ids.is_empty() && total + seconds > LONGEST {
            break;
        }
        total += seconds;
        ids.push(id);
    }
    Pick {
        scenes: ids,
        hook: board.title.trim().to_owned(),
    }
}

/// The storyboard of the Short: the picked scenes, framed 9:16, as one
/// chapter under the hook - the long video's chapter titles and on-screen
/// words left out, which would mean nothing a minute in.
pub fn board(long: &Storyboard, pick: &Pick) -> Storyboard {
    let mut scenes: Vec<_> = long
        .scenes()
        .filter(|scene| pick.scenes.contains(&scene.id))
        .cloned()
        .collect();
    for scene in &mut scenes {
        scene.on_screen_text = None;
    }
    if let Some(first) = scenes.first_mut() {
        first.on_screen_text = Some(pick.hook.clone()).filter(|hook| !hook.trim().is_empty());
    }
    let mood = long
        .chapters
        .iter()
        .find(|chapter| chapter.scenes.iter().any(|s| pick.scenes.contains(&s.id)))
        .map(|chapter| chapter.mood.clone())
        .unwrap_or_default();
    Storyboard {
        title: format!("{} - Short", long.title.trim()),
        aspect: "9:16".to_owned(),
        chapters: vec![Chapter {
            title: String::new(),
            mood,
            scenes,
            ..Chapter::default()
        }],
        ..long.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn long() -> Storyboard {
        let mut board: Storyboard = serde_json::from_str(
            r#"{"title":"Lost cities","aspect":"16:9","chapters":[
                {"title":"One","mood":"dawn","scenes":[
                    {"narration":"a","seconds":25,"onScreenText":"Chapter one","shots":[{"prompt":"x"}]},
                    {"narration":"b","seconds":25,"shots":[{"prompt":"y"}]}]},
                {"title":"Two","scenes":[
                    {"narration":"c","seconds":25,"shots":[{"prompt":"z"}]},
                    {"narration":"d","seconds":20,"shots":[{"prompt":"w"}]}]}]}"#,
        )
        .unwrap();
        board.tidy().unwrap();
        board
    }

    #[test]
    fn the_opening_minute_is_the_fallback() {
        let pick = opening(&long());
        assert_eq!(pick.scenes, ["c01s01", "c01s02"]);
        assert_eq!(pick.hook, "Lost cities");
    }

    #[test]
    fn a_pick_must_be_a_run_that_fits() {
        let board = long();
        let ids = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(fits(&board, &ids(&["c02s01", "c02s02"])));
        assert!(fits(&board, &ids(&["c01s02", "c02s01"])));
        assert!(!fits(&board, &ids(&["c01s01", "c02s01"])), "not in a row");
        assert!(
            !fits(&board, &ids(&["c01s01", "c01s02", "c02s01"])),
            "too long"
        );
        assert!(!fits(&board, &ids(&["c09s01"])));
        assert!(!fits(&board, &[]));
    }

    #[test]
    fn the_short_is_tall_one_chapter_under_its_hook() {
        let pick = Pick {
            scenes: vec!["c01s02".into(), "c02s01".into()],
            hook: "Why did they vanish?".into(),
        };
        let short = board(&long(), &pick);
        assert_eq!(short.aspect, "9:16");
        assert_eq!(short.frame(), (1080, 1920));
        assert_eq!(short.chapters.len(), 1);
        let ids: Vec<_> = short.scenes().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["c01s02", "c02s01"]);
        let words: Vec<_> = short.scenes().map(|s| s.on_screen_text.clone()).collect();
        assert_eq!(words, [Some("Why did they vanish?".into()), None]);
        assert_eq!(short.chapters[0].mood, "dawn");
        assert!(short.title.ends_with("- Short"));
    }
}
