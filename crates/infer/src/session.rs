//! ONNX Runtime session construction. CPU execution provider only.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ort::ep::CPU;
use ort::environment::Environment;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;

/// Errors from building an ONNX Runtime session.
#[derive(Debug)]
pub enum SessionError {
	/// The process-wide ONNX Runtime environment could not be initialised.
	Environment(ort::Error),
	/// The session for `path` could not be built (missing file, bad model, ...).
	Load { path: PathBuf, source: ort::Error },
}

impl fmt::Display for SessionError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Environment(e) => write!(f, "failed to initialise ONNX Runtime: {e}"),
			Self::Load { path, source } => {
				write!(f, "failed to build session for {}: {source}", path.display())
			}
		}
	}
}

impl std::error::Error for SessionError {}

/// ONNX Runtime allows one Environment per process, so it is created once and cached.
static ENVIRONMENT: Mutex<Option<Environment>> = Mutex::new(None);

fn environment() -> Result<Environment, SessionError> {
	let mut slot = ENVIRONMENT.lock().unwrap_or_else(|e| e.into_inner());
	if let Some(env) = slot.as_ref() {
		return Ok(env.clone());
	}
	let env = ort::init().build().map_err(SessionError::Environment)?;
	*slot = Some(env.clone());
	Ok(env)
}

/// Physical core count is not exposed portably, so it is estimated as logical / 2 (minimum 1).
/// This is a heuristic.
fn estimated_physical(logical: usize) -> usize {
	(logical / 2).max(1)
}

/// Intra-op thread count for a machine with `logical` logical cores: physical - 1, minimum 1.
pub fn threads_for(logical: usize) -> usize {
	estimated_physical(logical).saturating_sub(1).max(1)
}

/// Default intra-op thread count for this machine (heuristic, see [`threads_for`]).
pub fn default_threads() -> usize {
	let logical = std::thread::available_parallelism().map_or(1, |n| n.get());
	threads_for(logical)
}

/// Zero threads is invalid for ONNX Runtime, so clamp to at least 1.
fn clamp_threads(threads: usize) -> usize {
	threads.max(1)
}

/// Session settings beyond the thread count. [`Default`] is what [`build_session`] has always used.
#[derive(Debug, Clone, Copy)]
pub struct SessionOptions {
	pub optimization: GraphOptimizationLevel,
	pub memory_pattern: bool,
	pub cpu_arena: bool,
}

impl Default for SessionOptions {
	fn default() -> Self {
		Self { optimization: GraphOptimizationLevel::All, memory_pattern: true, cpu_arena: false }
	}
}

/// Build a CPU-only ONNX Runtime session from `path` with `threads` intra-op threads.
/// Returns the session and the thread count actually used (after clamping to at least 1).
pub fn build_session(path: &Path, threads: usize) -> Result<(Session, usize), SessionError> {
	build_session_with(path, threads, &SessionOptions::default())
}

/// [`build_session`] with explicit [`SessionOptions`].
pub fn build_session_with(
	path: &Path,
	threads: usize,
	opts: &SessionOptions,
) -> Result<(Session, usize), SessionError> {
	let threads = clamp_threads(threads);
	let env = environment()?;
	let load_err = |source: ort::Error| SessionError::Load {
		path: path.to_path_buf(),
		source,
	};
	let mut builder = Session::builder(&env)
		.map_err(load_err)?
		.with_execution_providers([CPU::default().with_arena_allocator(opts.cpu_arena).build()])
		.map_err(|e| load_err(e.into()))?
		.with_optimization_level(opts.optimization)
		.map_err(|e| load_err(e.into()))?
		.with_memory_pattern(opts.memory_pattern)
		.map_err(|e| load_err(e.into()))?
		.with_intra_threads(threads)
		.map_err(|e| load_err(e.into()))?
		// Frames arrive every 33-66 ms; spinning pool threads only burn CPU between them.
		.with_intra_op_spinning(false)
		.map_err(|e| load_err(e.into()))?
		.with_inter_op_spinning(false)
		.map_err(|e| load_err(e.into()))?;
	let session = builder.commit_from_file(path).map_err(load_err)?;
	Ok((session, threads))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn threads_for_counts() {
		assert_eq!(threads_for(1), 1);
		assert_eq!(threads_for(2), 1);
		assert_eq!(threads_for(4), 1);
		assert_eq!(threads_for(8), 3);
		assert_eq!(threads_for(16), 7);
	}

	#[test]
	fn default_threads_is_at_least_one() {
		assert!(default_threads() >= 1);
	}

	#[test]
	fn zero_threads_clamped_to_one() {
		assert_eq!(clamp_threads(0), 1);
		assert_eq!(clamp_threads(3), 3);
	}

	#[test]
	fn invalid_path_error_names_path() {
		let Err(err) = build_session(Path::new("missing_face_model.onnx"), 1) else {
			panic!("missing model must fail");
		};
		assert!(err.to_string().contains("missing_face_model.onnx"));
	}
}
