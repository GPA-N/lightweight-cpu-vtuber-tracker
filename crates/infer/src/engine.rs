//! Inference engine abstraction: one f32 tensor in, named f32 tensors out.
//!
//! Two backends: ONNX Runtime (`ort`, the existing path) and OpenVINO (crate `openvino`, runtime
//! loaded, CPU plugin only). The engine is chosen at start-up by [`resolve_engine`], which reads
//! `VT_ENGINE=ort|openvino|auto` (unset means ort).

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

/// Parses an engine name. `None` and `""` give ONNX Runtime; `auto` gives `Ok(None)` (decide by availability).
pub fn parse_engine(name: Option<&str>) -> Result<Option<Engine>, String> {
	match name.map(|n| n.trim().to_ascii_lowercase()).as_deref() {
		None | Some("") => Ok(Some(Engine::Ort)),
		Some("auto") => Ok(None),
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

/// Picks the engine from `VT_ENGINE`. Unset means ort, with no OpenVINO probe. `openvino` and `auto`
/// use OpenVINO when its libraries load, else ort with a log line. An unknown value logs and behaves
/// as auto.
pub fn resolve_engine() -> Engine {
	let value = std::env::var("VT_ENGINE").ok();
	let choice = parse_engine(value.as_deref()).unwrap_or_else(|e| {
		eprintln!("VT_ENGINE: {e}; using auto");
		None
	});
	match choice {
		Some(Engine::Ort) => Engine::Ort,
		_ if openvino_available() => Engine::OpenVino,
		_ => {
			eprintln!("engine: OpenVINO libraries not found, falling back to ONNX Runtime");
			Engine::Ort
		}
	}
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
		ort_outputs(&outputs, &self.output_names)
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
		ort_outputs(&outputs, &self.output_names)
	}
}

/// Copies the named outputs of an ONNX Runtime run into owned tensors.
fn ort_outputs(outputs: &ort::session::SessionOutputs<'_>, names: &[String]) -> Result<Vec<NamedOutput>, EngineError> {
	names
		.iter()
		.map(|name| {
			let (s, d) = outputs[name.as_str()]
				.try_extract_tensor::<f32>()
				.map_err(|e| EngineError::Run(e.to_string()))?;
			Ok(NamedOutput { name: name.clone(), shape: s.iter().map(|&x| x as usize).collect(), data: d.to_vec() })
		})
		.collect()
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

/// Experiment switches from `VT_OV_OPTS` (comma list). Unset means none, which is the default path.
/// CPU pinning is on unless `nopin` is listed.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct OvOpts {
	static_shapes: bool,
	streams1: bool,
	nosmt: bool,
	f32: bool,
	nopin: bool,
	rtcache0: bool,
}

impl OvOpts {
	fn from_env() -> Result<Self, EngineError> {
		let mut o = Self::default();
		let Ok(list) = std::env::var("VT_OV_OPTS") else { return Ok(o) };
		for tok in list.split(',').map(str::trim).filter(|t| !t.is_empty()) {
			match tok {
				"static" => o.static_shapes = true,
				"streams1" => o.streams1 = true,
				"nosmt" => o.nosmt = true,
				"f32" => o.f32 = true,
				"nopin" => o.nopin = true,
				"rtcache0" => o.rtcache0 = true,
				other => return Err(EngineError::Load(format!("VT_OV_OPTS: unknown option {other}"))),
			}
		}
		Ok(o)
	}

	fn apply(&self, core: &mut openvino::Core) -> Result<(), EngineError> {
		use openvino::{DeviceType, RwPropertyKey as K};
		let mut set = |key: &K, value: &str| core.set_property(&DeviceType::CPU, key, value).map_err(ov_load);
		if self.streams1 {
			set(&K::NumStreams, "1")?;
		}
		if self.nosmt {
			set(&K::HintEnableHyperThreading, "NO")?;
		}
		if self.f32 {
			set(&K::HintInferencePrecision, "f32")?;
		}
		if !self.nopin {
			set(&K::HintEnableCpuPinning, "YES")?;
		}
		if self.rtcache0 {
			set(&K::Other("CPU_RUNTIME_CACHE_CAPACITY".into()), "0")?;
		}
		Ok(())
	}
}

/// Fixes the two inputs of the body model to the shapes the app always feeds.
fn reshape_static(model: &mut openvino::Model) -> Result<(), EngineError> {
	use openvino::PartialShape;
	let image = PartialShape::new_static(4, &[1, 3, 224, 224]).map_err(ov_load)?;
	let cond = PartialShape::new_static(2, &[1, 3]).map_err(ov_load)?;
	model.reshape(&[("image", &image), ("cliff_cond", &cond)]).map_err(ov_load)
}

/// Logs the process working set (MB) with a stage name.
fn log_memory(stage: &str) {
	match timing::memory::process_memory() {
		Ok(m) => eprintln!("[openvino] {stage}: working set {:.1} MB, peak {:.1} MB", m.working_set_mb, m.peak_working_set_mb),
		Err(e) => eprintln!("[openvino] {stage}: memory unavailable: {e}"),
	}
}

impl OvModel {
	pub fn load(path: &Path, threads: usize) -> Result<Self, EngineError> {
		use openvino::{DeviceType, RwPropertyKey};
		let opts = OvOpts::from_env()?;
		let mut core = openvino::Core::new().map_err(ov_load)?;
		let threads = threads.max(1).to_string();
		core.set_property(&DeviceType::CPU, &RwPropertyKey::HintPerformanceMode, "LATENCY")
			.map_err(ov_load)?;
		core.set_property(&DeviceType::CPU, &RwPropertyKey::InferenceNumThreads, &threads)
			.map_err(ov_load)?;
		opts.apply(&mut core)?;
		let path_str = path.to_str().ok_or_else(|| ov_load("model path is not UTF-8"))?;
		let mut model = core.read_model_from_file(path_str, "").map_err(|e| ov_load(format!("{path_str}: {e}")))?;
		if opts.static_shapes {
			reshape_static(&mut model)?;
		}
		log_memory("before compile");
		let mut compiled = core.compile_model(&model, DeviceType::CPU).map_err(ov_load)?;
		log_memory("after compile");
		let n = compiled.get_output_size().map_err(ov_load)?;
		let output_names = (0..n)
			.map(|i| compiled.get_output_by_index(i).and_then(|o| o.get_name()))
			.collect::<Result<Vec<_>, _>>()
			.map_err(ov_load)?;
		let request = compiled.create_infer_request().map_err(ov_load)?;
		Ok(Self { _core: core, request, output_names })
	}
}

/// Maps an OpenVINO run error to [`EngineError::Run`].
fn ov_run(e: openvino::InferenceError) -> EngineError {
	EngineError::Run(e.to_string())
}

/// Builds an OpenVINO f32 tensor of the given shape from row-major data.
fn ov_tensor(data: &[f32], shape: &[usize]) -> Result<openvino::Tensor, EngineError> {
	use openvino::{ElementType, Shape};
	let dims: Vec<i64> = shape.iter().map(|&d| d as i64).collect();
	let mut tensor = openvino::Tensor::new(ElementType::F32, &Shape::new(&dims).map_err(ov_run)?).map_err(ov_run)?;
	tensor.get_data_mut::<f32>().map_err(ov_run)?.copy_from_slice(data);
	Ok(tensor)
}

/// Copies the outputs of the last OpenVINO inference into owned tensors, in `names` order.
fn ov_outputs(request: &mut openvino::InferRequest, names: &[String]) -> Result<Vec<NamedOutput>, EngineError> {
	let mut outs = Vec::with_capacity(names.len());
	for (i, name) in names.iter().enumerate() {
		let t = request.get_output_tensor_by_index(i).map_err(ov_run)?;
		let shape = t.get_shape().map_err(ov_run)?.get_dimensions().iter().map(|&d| d as usize).collect();
		outs.push(NamedOutput { name: name.clone(), shape, data: t.get_data::<f32>().map_err(ov_run)?.to_vec() });
	}
	Ok(outs)
}

impl ModelRun for OvModel {
	fn run(&mut self, input: &[f32], shape: &[usize]) -> Result<Vec<NamedOutput>, EngineError> {
		check_len(input, shape)?;
		let tensor = ov_tensor(input, shape)?;
		self.request.set_input_tensor(&tensor).map_err(ov_run)?;
		self.request.infer().map_err(ov_run)?;
		ov_outputs(&mut self.request, &self.output_names)
	}

	fn run_two(&mut self, a: NamedInput, b: NamedInput) -> Result<Vec<NamedOutput>, EngineError> {
		check_len(a.data, a.shape)?;
		check_len(b.data, b.shape)?;
		for i in [a, b] {
			let tensor = ov_tensor(i.data, i.shape)?;
			self.request.set_tensor(i.name, &tensor).map_err(ov_run)?;
		}
		self.request.infer().map_err(ov_run)?;
		ov_outputs(&mut self.request, &self.output_names)
	}
}
