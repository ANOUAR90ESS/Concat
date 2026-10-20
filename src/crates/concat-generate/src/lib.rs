// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors

//! Long videos from an idea: the script, the prompts, the voice and the
//! shots, and the cut.
//!
//! Four steps, each leaving files the next one reads, so each can be run
//! again, or its files replaced by hand, without the others noticing:
//!
//! 1. [`writer`]: Claude writes a [`storyboard::Storyboard`] chapter by
//!    chapter, to the shape [`budget`] works out from the length - so many
//!    scenes, each a line of narration with how it is said, and so many
//!    shots, each a still's image prompt or a clip's picture and motion -
//!    into the job's folder, with a prompt sheet beside it.
//! 2. [`job::Job::make`]: the narration is read and the shots are made,
//!    by the [`providers`] chosen: the window's own voices or ElevenLabs,
//!    Replicate's video models, colour cards for a draft, or files made by
//!    hand.
//! 3. [`plan::plan`]: where every file lands, worked out from their
//!    lengths: each scene as long as its narration, its shots sharing it.
//! 4. [`plan::cut`]: the plan laid down as the edit's own commands on a
//!    new timeline - cross-fades, captions, titles, a music bed, moves on
//!    stills, fades - so the result is undone, edited and exported like
//!    anything else.
//!
//! The crate draws nothing. The window's Long video sheet is its face, and
//! runs steps 1 and 2 on a worker and step 4 on the window's session.

pub mod budget;
pub mod job;
pub mod plan;
pub mod providers;
pub mod storyboard;
pub mod writer;
