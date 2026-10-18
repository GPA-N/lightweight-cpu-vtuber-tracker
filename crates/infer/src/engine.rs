//! Inference engine abstraction: one f32 tensor in, named f32 tensors out.
//!
//! Two backends: ONNX Runtime (`ort`, the existing path) and OpenVINO (crate `openvino`, runtime
//! loaded, CPU plugin only). The engine is chosen at start-up by [`resolve_engine`], which reads
//! `VT_ENGINE=ort|openvino|auto`.

use std::fmt;
use std::path::Path;

use ort::session::Session;
use ort::value::Tensor;

use crate::session::{build_session, SessionError};

/// One output tensor of a model run.
#[derive(Debug, Clone, PartialEq)]
pub struct NamedOutput {
	pub name: String,
	pub shape: Vec<usize>,
	pub data: Vec<f32>,
}

/// Errors from either backend.
#[derive(Debug)]
pub enum EngineError {
	/// The model could not be loaded or compiled.
	Load(String),
	/// The input length does not match the shape, or the shape does not match the model.
	Input(String),
	/// The run itself failed.
	Run(String),
}

impl fmt::Display for EngineError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Load(m) => write!(f, "engine load: {m}"),
			Self::Input(m) => write!(f, "engine input: {m}"),
			Self::Run(m) => write!(f, "engine run: {m}"),
		}
	}
}

impl std::error::Error for EngineError {}

impl From<SessionError> for EngineError {
	fn from(e: SessionError) -> Self {
		Self::Load(e.to_string())
	}
}

/// One named input of a multi-input model: a row-major f32 tensor.
#[derive(Debug, Clone, Copy)]
pub struct NamedInput<'a> {
	pub name: &'a str,
	pub data: &'a [f32],
	pub shape: &'a [usize],
}

/// Runs a single-input model. The input is a row-major f32 tensor of the given shape.
pub trait ModelRun {
	fn run(&mut self, input: &[f32], shape: &[usize]) -> Result<Vec<NamedOutput>, EngineError>;

	/// Runs a model that takes two named inputs. Backends that cannot do it say so.
	fn run_two(&mut self, _a: NamedInput, _b: NamedInput) -> Result<Vec<NamedOutput>, EngineError> {
		Err(EngineError::Input("this backend runs single-input models only".into()))
	}
}

/// Which backend to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
	Ort,
	OpenVino,
}

/// Parses an engine name. `None` and `auto` give `Ok(None)` (decide by availability).
pub fn parse_engine(name: Option<&str>) -> Result<Option<Engine>, String> {
	match name.map(|n| n.trim().to_ascii_lowercase()).as_deref() {
		None | Some("") | Some("auto") => Ok(None),
		Some("ort") => Ok(Some(Engine::Ort)),
		Some("openvino") => Ok(Some(Engine::OpenVino)),
		Some(other) => Err(format!("unknown engine '{other}', expected ort, openvino or auto")),
	}
}

/// True when the OpenVINO runtime libraries can be found and loaded.
pub fn openvino_available() -> bool {
	add_exe_dir_to_path();
	openvino::Core::new().is_ok()
}

/// The runtime is found through PATH, so a bundle that ships the DLLs next to the executable needs
/// that folder on PATH. Idempotent; only touches PATH when `openvino_c.dll` is there.
fn add_exe_dir_to_path() {
	let Some(dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)) else { return };
	if !dir.join("openvino_c.dll").exists() {
		return;
	}
	let mut paths: Vec<_> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
	if paths.contains(&dir) {
		return;
	}
	paths.insert(0, dir);
	if let Ok(joined) = std::env::join_paths(paths) {
		// SAFETY: called from engine selection at start-up, before any thread reads the environment
		// through this process's own code (the OpenVINO loader reads it right after).
		unsafe { std::env::set_var("PATH", joined) };
	}
}

/// Picks the engine from `VT_ENGINE`. Auto means OpenVINO when its libraries load, else ort with a
/// log line. An unknown value logs and behaves as auto. An explicit `openvino` is honoured even if
/// the libraries are missing; loading a model then fails with a clear error.
pub fn resolve_engine() -> Engine {
	let value = std::env::var("VT_ENGINE").ok();
	let choice = parse_engine(value.as_deref()).unwrap_or_else(|e| {
		eprintln!("VT_ENGINE: {e}; using auto");
		None
	});
	choice.unwrap_or_else(|| {
		if openvino_available() {
			Engine::OpenVino
		} else {
			eprintln!("engine: OpenVINO libraries not found, falling back to ONNX Runtime");
			Engine::Ort
		}
	})
}

/// Loads `path` on the chosen engine with `threads` inference threads.
pub fn load_model(
	engine: Engine,
	path: &Path,
	threads: usize,
) -> Result<Box<dyn ModelRun + Send>, EngineError> {
	Ok(match engine {
		Engine::Ort => Box::new(OrtModel::load(path, threads)?),
		Engine::OpenVino => Box::new(OvModel::load(path, threads)?),
	})
}

/// ONNX Runtime backend: wraps the session built by [`build_session`].
pub struct OrtModel {
	session: Session,
	input_name: String,
	output_names: Vec<String>,
}

impl OrtModel {
	pub fn load(path: &Path, threads: usize) -> Result<Self, EngineError> {
		let (session, _) = build_session(path, threads)?;
		Ok(Self::new(session))
	}

	pub fn new(session: Session) -> Self {
		let input_name = session.inputs().first().map(|i| i.name().to_string()).unwrap_or_default();
		let output_names = session.outputs().iter().map(|o| o.name().to_string()).collect();
		Self { session, input_name, output_names }
	}
}

impl ModelRun for OrtModel {
	fn run(&mut self, input: &[f32], shape: &[usize]) -> Result<Vec<NamedOutput>, EngineError> {
		check_len(input, shape)?;
		let tensor = Tensor::<f32>::from_array((shape.to_vec(), input.to_vec()))
			.map_err(|e| EngineError::Input(e.to_string()))?;
		let outputs = self
			.session
			.run(ort::inputs![self.input_name.as_str() => tensor])
			.map_err(|e| EngineError::Run(e.to_string()))?;
		self.output_names
			.iter()
			.map(|name| {
				let (s, d) = outputs[name.as_str()]
					.try_extract_tensor::<f32>()
					.map_err(|e| EngineError::Run(e.to_string()))?;
				Ok(NamedOutput {
					name: name.clone(),
					shape: s.iter().map(|&x| x as usize).collect(),
					data: d.to_vec(),
				})
			})
			.collect()
	}

	fn run_two(&mut self, a: NamedInput, b: NamedInput) -> Result<Vec<NamedOutput>, EngineError> {
		check_len(a.data, a.shape)?;
		check_len(b.data, b.shape)?;
		let to_tensor = |i: NamedInput| {
			Tensor::<f32>::from_array((i.shape.to_vec(), i.data.to_vec())).map_err(|e| EngineError::Input(e.to_string()))
		};
		let (ta, tb) = (to_tensor(a)?, to_tensor(b)?);
		let outputs = self
			.session
			.run(ort::inputs![a.name => ta, b.name => tb])
			.map_err(|e| EngineError::Run(e.to_string()))?;
		self.output_names
			.iter()
			.map(|name| {
				let (s, d) = outputs[name.as_str()]
					.try_extract_tensor::<f32>()
					.map_err(|e| EngineError::Run(e.to_string()))?;
				Ok(NamedOutput { name: name.clone(), shape: s.iter().map(|&x| x as usize).collect(), data: d.to_vec() })
			})
			.collect()
	}
}

fn check_len(input: &[f32], shape: &[usize]) -> Result<(), EngineError> {
	let want: usize = shape.iter().product();
	if input.len() == want {
		Ok(())
	} else {
		Err(EngineError::Input(format!("{} values for shape {shape:?} ({want} expected)", input.len())))
	}
}

/// OpenVINO backend: CPU plugin, latency hint, one infer request.
pub struct OvModel {
	// Kept alive for the lifetime of the compiled model.
	_core: openvino::Core,
	request: openvino::InferRequest,
	output_names: Vec<String>,
}

fn ov_load(e: impl fmt::Display) -> EngineError {
	EngineError::Load(e.to_string())
}

impl OvModel {
	pub fn load(path: &Path, threads: usize) -> Result<Self, EngineError> {
		use openvino::{DeviceType, RwPropertyKey};
		let mut core = openvino::Core::new().map_err(ov_load)?;
		let threads = threads.max(1).to_string();
		core.set_property(&DeviceType::CPU, &RwPropertyKey::HintPerformanceMode, "LATENCY")
			.map_err(ov_load)?;
		core.set_property(&DeviceType::CPU, &RwPropertyKey::InferenceNumThreads, &threads)
			.map_err(ov_load)?;
		let path_str = path.to_str().ok_or_else(|| ov_load("model path is not UTF-8"))?;
		let model = core.read_model_from_file(path_str, "").map_err(|e| ov_load(format!("{path_str}: {e}")))?;
		let mut compiled = core.compile_model(&model, DeviceType::CPU).map_err(ov_load)?;
		let n = compiled.get_output_size().map_err(ov_load)?;
		let output_names = (0..n)
			.map(|i| compiled.get_output_by_index(i).and_then(|o| o.get_name()))
			.collect::<Result<Vec<_>, _>>()
			.map_err(ov_load)?;
		let request = compiled.create_infer_request().map_err(ov_load)?;
		Ok(Self { _core: core, request, output_names })
	}
}

impl ModelRun for OvModel {
	fn run(&mut self, input: &[f32], shape: &[usize]) -> Result<Vec<NamedOutput>, EngineError> {
		use openvino::{ElementType, Shape, Tensor};
		check_len(input, shape)?;
		let run = |e: openvino::InferenceError| EngineError::Run(e.to_string());
		let dims: Vec<i64> = shape.iter().map(|&d| d as i64).collect();
		let mut tensor = Tensor::new(ElementType::F32, &Shape::new(&dims).map_err(run)?).map_err(run)?;
		tensor.get_data_mut::<f32>().map_err(run)?.copy_from_slice(input);
		self.request.set_input_tensor(&tensor).map_err(run)?;
		self.request.infer().map_err(run)?;
		let mut outs = Vec::with_capacity(self.output_names.len());
		for (i, name) in self.output_names.iter().enumerate() {
			let t = self.request.get_output_tensor_by_index(i).map_err(run)?;
			let shape = t.get_shape().map_err(run)?.get_dimensions().iter().map(|&d| d as usize).collect();
			outs.push(NamedOutput { name: name.clone(), shape, data: t.get_data::<f32>().map_err(run)?.to_vec() });
		}
		Ok(outs)
	}

	fn run_two(&mut self, a: NamedInput, b: NamedInput) -> Result<Vec<NamedOutput>, EngineError> {
		use openvino::{ElementType, Shape, Tensor};
		check_len(a.data, a.shape)?;
		check_len(b.data, b.shape)?;
		let run = |e: openvino::InferenceError| EngineError::Run(e.to_string());
		for i in [a, b] {
			let dims: Vec<i64> = i.shape.iter().map(|&d| d as i64).collect();
			let mut tensor = Tensor::new(ElementType::F32, &Shape::new(&dims).map_err(run)?).map_err(run)?;
			tensor.get_data_mut::<f32>().map_err(run)?.copy_from_slice(i.data);
			self.request.set_tensor(i.name, &tensor).map_err(run)?;
		}
		self.request.infer().map_err(run)?;
		let mut outs = Vec::with_capacity(self.output_names.len());
		for (i, name) in self.output_names.iter().enumerate() {
			let t = self.request.get_output_tensor_by_index(i).map_err(run)?;
			let shape = t.get_shape().map_err(run)?.get_dimensions().iter().map(|&d| d as usize).collect();
			outs.push(NamedOutput { name: name.clone(), shape, data: t.get_data::<f32>().map_err(run)?.to_vec() });
		}
		Ok(outs)
	}
}
