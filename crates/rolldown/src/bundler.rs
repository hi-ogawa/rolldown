use super::stages::{
  link_stage::{LinkStage, LinkStageOutput},
  scan_stage::ScanStageOutput,
};
use crate::{
  bundler_builder::BundlerBuilder,
  module_loader::hmr_module_loader::HmrModuleLoader,
  stages::{
    generate_stage::{render_hmr_chunk::render_hmr_chunk, GenerateStage},
    scan_stage::ScanStage,
  },
  type_alias::IndexEcmaAst,
  types::bundle_output::BundleOutput,
  watcher::watcher::{wait_for_change, Watcher},
  BundlerOptions, SharedOptions, SharedResolver,
};
use anyhow::Result;
use arcstr::ArcStr;

use rolldown_common::{
  ModuleIdx, ModuleTable, NormalizedBundlerOptions, SharedFileEmitter, SymbolRefDb,
};

use rolldown_error::{BuildDiagnostic, BuildResult};
use rolldown_fs::{FileSystem, OsFileSystem};
use rolldown_plugin::{
  HookBuildEndArgs, HookRenderErrorArgs, SharedPluginDriver, __inner::SharedPluginable,
};
use rolldown_std_utils::OptionExt;
use rustc_hash::FxHashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing_chrome::FlushGuard;

pub struct Bundler {
  pub closed: bool,
  pub(crate) fs: OsFileSystem,
  pub(crate) options: SharedOptions,
  pub(crate) resolver: SharedResolver,
  pub(crate) file_emitter: SharedFileEmitter,
  pub(crate) plugin_driver: SharedPluginDriver,
  pub(crate) warnings: Vec<BuildDiagnostic>,
  pub(crate) _log_guard: Option<FlushGuard>,
  pub(crate) previous_module_table: ModuleTable,
  pub(crate) previous_module_id_to_modules: FxHashMap<ArcStr, ModuleIdx>,
  pub(crate) pervious_index_ecma_ast: IndexEcmaAst,
  pub(crate) pervious_symbols: SymbolRefDb,
}

impl Bundler {
  pub fn new(options: BundlerOptions) -> Self {
    BundlerBuilder::default().with_options(options).build()
  }

  pub fn with_plugins(options: BundlerOptions, plugins: Vec<SharedPluginable>) -> Self {
    BundlerBuilder::default().with_options(options).with_plugins(plugins).build()
  }
}

impl Bundler {
  #[tracing::instrument(level = "debug", skip_all)]
  pub async fn write(&mut self) -> BuildResult<BundleOutput> {
    let mut output = self.bundle_up(/* is_write */ true).await?;

    self.write_file_to_disk(&output)?;

    self.plugin_driver.write_bundle(&mut output.assets, &self.options).await?;

    output.warnings.append(&mut self.warnings);

    Ok(output)
  }

  #[tracing::instrument(level = "debug", skip_all)]
  pub async fn generate(&mut self) -> BuildResult<BundleOutput> {
    self.bundle_up(/* is_write */ false).await.map(|mut output| {
      output.warnings.append(&mut self.warnings);
      output
    })
  }

  #[tracing::instrument(level = "debug", skip_all)]
  pub async fn close(&mut self) -> Result<()> {
    if self.closed {
      return Ok(());
    }

    self.closed = true;
    self.plugin_driver.close_bundle().await?;

    Ok(())
  }

  pub async fn scan(&mut self) -> BuildResult<ScanStageOutput> {
    self.plugin_driver.build_start(&self.options).await?;

    let mut error_for_build_end_hook = None;

    let scan_stage_output = match ScanStage::new(
      Arc::clone(&self.options),
      Arc::clone(&self.plugin_driver),
      self.fs,
      Arc::clone(&self.resolver),
    )
    .scan()
    .await
    {
      Ok(v) => v,
      Err(errs) => {
        // TODO: So far we even call build end hooks on unhandleable errors . But should we call build end hook even for unhandleable errors?
        error_for_build_end_hook = Some(errs.first().unpack_ref().to_string());
        self
          .plugin_driver
          .build_end(error_for_build_end_hook.map(|error| HookBuildEndArgs { error }).as_ref())
          .await?;
        self.plugin_driver.close_bundle().await?;
        return Err(errs);
      }
    };

    self
      .plugin_driver
      .build_end(error_for_build_end_hook.map(|error| HookBuildEndArgs { error }).as_ref())
      .await?;

    Ok(scan_stage_output)
  }

  #[allow(clippy::unused_async)]
  pub async fn hmr_rebuild(&mut self, changed_files: Vec<String>) -> BuildResult<BundleOutput> {
    let hmr_module_loader = HmrModuleLoader::new(
      Arc::clone(&self.options),
      Arc::clone(&self.plugin_driver),
      self.fs,
      Arc::clone(&self.resolver),
      std::mem::take(&mut self.previous_module_id_to_modules),
      std::mem::take(&mut self.previous_module_table),
      std::mem::take(&mut self.pervious_index_ecma_ast),
      std::mem::take(&mut self.pervious_symbols),
    )?;

    let mut hmr_module_loader_output = hmr_module_loader.fetch_changed_files(changed_files).await?;

    let output = render_hmr_chunk(&self.options, &mut hmr_module_loader_output);

    self.write_file_to_disk(&output)?;

    // store last build modules info
    self.previous_module_table = hmr_module_loader_output.module_table;
    self.previous_module_id_to_modules = hmr_module_loader_output.module_id_to_modules;
    self.pervious_index_ecma_ast = hmr_module_loader_output.index_ecma_ast;
    self.pervious_symbols = hmr_module_loader_output.symbol_ref_db;

    Ok(output)
  }

  fn write_file_to_disk(&self, output: &BundleOutput) -> Result<()> {
    let dir = self.options.cwd.join(&self.options.dir);

    self.fs.create_dir_all(&dir).map_err(|err| {
      anyhow::anyhow!("Could not create directory for output chunks: {:?}", dir).context(err)
    })?;

    for chunk in &output.assets {
      let dest = dir.join(chunk.filename());
      if let Some(p) = dest.parent() {
        if !self.fs.exists(p) {
          self.fs.create_dir_all(p).unwrap();
        }
      };
      self
        .fs
        .write(&dest, chunk.content_as_bytes())
        .map_err(|err| anyhow::anyhow!("Failed to write file in {:?}", dest).context(err))?;
    }

    Ok(())
  }

  async fn try_build(&mut self) -> BuildResult<LinkStageOutput> {
    let build_info = self.scan().await?;
    Ok(LinkStage::new(build_info, &self.options).link())
  }

  #[allow(clippy::missing_transmute_annotations)]
  async fn bundle_up(&mut self, is_write: bool) -> BuildResult<BundleOutput> {
    if self.closed {
      return Err(
        anyhow::anyhow!(
          "Bundle is already closed, no more calls to 'generate' or 'write' are allowed."
        )
        .into(),
      );
    }

    let mut link_stage_output = self.try_build().await?;

    self.plugin_driver.render_start(&self.options).await?;

    let bundle_output =
      GenerateStage::new(&mut link_stage_output, &self.options, &self.plugin_driver)
        .generate()
        .await; // Notice we don't use `?` to break the control flow here.

    if let Err(errs) = &bundle_output {
      self
        .plugin_driver
        .render_error(&HookRenderErrorArgs { error: errs.first().unpack_ref().to_string() })
        .await?;
    }

    let mut output = bundle_output?;

    // Add additional files from build plugins.
    self.file_emitter.add_additional_files(&mut output.assets);

    self.plugin_driver.generate_bundle(&mut output.assets, is_write, &self.options).await?;

    output.watch_files = self.plugin_driver.watch_files.iter().map(|f| f.clone()).collect();

    // store last build modules info
    self.previous_module_table = link_stage_output.module_table;
    self.previous_module_id_to_modules = link_stage_output.module_id_to_modules;
    self.pervious_index_ecma_ast = link_stage_output.ast_table;
    self.pervious_symbols = link_stage_output.symbol_db;

    Ok(output)
  }

  pub fn options(&self) -> &NormalizedBundlerOptions {
    &self.options
  }

  pub fn watch(bundler: Arc<Mutex<Bundler>>) -> Result<Arc<Watcher>> {
    let watcher = Arc::new(Watcher::new(bundler)?);

    wait_for_change(Arc::clone(&watcher));

    Ok(watcher)
  }
}

fn _test_bundler() {
  #[allow(clippy::needless_pass_by_value)]
  fn _assert_send(_foo: impl Send) {}
  let mut bundler = Bundler::new(BundlerOptions::default());
  let write_fut = bundler.write();
  _assert_send(write_fut);
  let mut bundler = Bundler::new(BundlerOptions::default());
  let generate_fut = bundler.generate();
  _assert_send(generate_fut);
}
