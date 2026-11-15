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

/// The minute picked: which scenes, in order, the line over its start and
/// the Short's own title.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pick {
    /// The scenes' ids, a run of them in a row.
    pub scenes: Vec<String>,
    /// A few words over the first second, to stop the thumb.
    pub hook: String,
    /// The Short's title, its own and not the long video's: its timeline
    /// is named by it, and it goes up with it. Empty for the long video's
    /// title with "Short" after it.
    pub title: String,
}

/// `board` with each scene as long as the cut lays it out from what was
/// made for it: the voice as read, with the pad after it and never less
/// than the shortest scene, by the same sums as [`crate::plan::timing`] -
/// not the written seconds, which run long or short by a good part. What
/// a Short's minute is measured in. A scene `made` holds no voice for
/// keeps its written seconds.
pub fn timed(
    board: &Storyboard,
    made: &std::collections::HashMap<String, crate::plan::SceneFiles>,
) -> Storyboard {
    let options = crate::plan::Options::default();
    let mut board = board.clone();
    for chapter in &mut board.chapters {
        for scene in &mut chapter.scenes {
            if let Some(files) = made.get(&scene.id).filter(|files| files.voice.is_some()) {
                let length = crate::plan::timing(scene, files, &options).length;
                if length.is_finite() && length > 0.0 {
                    scene.seconds = length;
                }
            }
        }
    }
    board
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
        title: String::new(),
    }
}

/// `picks` made up to `count` with runs of the scenes no pick holds, each
/// as many scenes in a row as fit in [`LONGEST`], from the start of the
/// video on: the writer may pick fewer than asked, or picks that do not
/// fit, and the owner asked for three Shorts and got one. The hook of one
/// made up here is the first of its scenes' stickers, else the title.
pub fn filled(board: &Storyboard, mut picks: Vec<Pick>, count: usize) -> Vec<Pick> {
    let taken: std::collections::HashSet<String> =
        picks.iter().flat_map(|pick| pick.scenes.clone()).collect();
    let all: Vec<_> = board.scenes().collect();
    let mut run: Vec<String> = Vec::new();
    let mut total = 0.0;
    let close = |run: &mut Vec<String>, total: &mut f64, picks: &mut Vec<Pick>| {
        if !run.is_empty() && picks.len() < count {
            let hook = all
                .iter()
                .filter(|scene| run.contains(&scene.id))
                .find_map(|scene| scene.sticker.clone())
                .unwrap_or_else(|| board.title.trim().to_owned());
            picks.push(Pick {
                scenes: std::mem::take(run),
                hook,
                title: String::new(),
            });
        }
        run.clear();
        *total = 0.0;
    };
    for scene in &all {
        if picks.len() >= count {
            break;
        }
        if taken.contains(&scene.id) {
            close(&mut run, &mut total, &mut picks);
            continue;
        }
        let seconds = scene.seconds.max(0.0);
        if !run.is_empty() && total + seconds > LONGEST {
            close(&mut run, &mut total, &mut picks);
        }
        total += seconds;
        run.push(scene.id.clone());
    }
    close(&mut run, &mut total, &mut picks);
    picks
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
        title: match pick.title.trim() {
            "" => format!("{} - Short", long.title.trim()),
            title => title.to_owned(),
        },
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

    /// Fewer picks than asked are made up with runs of the scenes left,
    /// none sharing a scene with a pick, each fitting a Short.
    #[test]
    fn fewer_picks_than_asked_are_made_up() {
        let board = long();
        let picked = Pick {
            scenes: vec!["c01s02".into()],
            hook: "Picked".into(),
            ..Pick::default()
        };
        let picks = filled(&board, vec![picked], 3);
        assert_eq!(picks.len(), 3);
        assert_eq!(picks[0].hook, "Picked");
        assert_eq!(picks[1].scenes, ["c01s01"]);
        assert_eq!(picks[2].scenes, ["c02s01", "c02s02"]);
        for pick in &picks {
            assert!(fits(&board, &pick.scenes), "{pick:?}");
        }
        // From nothing, as many as the video holds.
        assert_eq!(filled(&board, Vec::new(), 3).len(), 2);
        assert_eq!(filled(&board, Vec::new(), 1).len(), 1);
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
        let mut pick = Pick {
            scenes: vec!["c01s02".into(), "c02s01".into()],
            hook: "Why did they vanish?".into(),
            ..Pick::default()
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
        pick.title = "The city that drowned in a night".into();
        assert_eq!(
            board(&long(), &pick).title,
            "The city that drowned in a night"
        );
    }

    /// A Short's minute is the voice's: a scene read longer than written
    /// no longer fits where its written seconds did.
    #[test]
    fn the_voice_s_seconds_measure_a_short() {
        let board = long();
        let ids = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(fits(&board, &ids(&["c02s01", "c02s02"])));
        let voiced = |seconds: f64| crate::plan::SceneFiles {
            voice: Some(("/v.mp3".to_owned(), seconds)),
            ..crate::plan::SceneFiles::default()
        };
        let made = std::collections::HashMap::from([
            ("c02s01".to_owned(), voiced(38.0)),
            ("c02s02".to_owned(), voiced(31.0)),
            // Read in a second: the cut still gives it the shortest scene.
            ("c01s02".to_owned(), voiced(1.0)),
            ("c01s01".to_owned(), crate::plan::SceneFiles::default()),
        ]);
        let timed = timed(&board, &made);
        assert!(!fits(&timed, &ids(&["c02s01", "c02s02"])));
        let options = crate::plan::Options::default();
        let seconds: Vec<f64> = timed.scenes().map(|s| s.seconds).collect();
        let padded = |s: f64| (s + options.scene_pad).max(options.min_scene);
        assert_eq!(seconds, [25.0, padded(1.0), padded(38.0), padded(31.0)]);
    }
}
