use super::module_task::{ModuleTask, ModuleTaskOwner};
use super::task_context::TaskContextMeta;
use crate::module_loader::task_context::TaskContext;
use crate::type_alias::IndexEcmaAst;
use arcstr::ArcStr;
use oxc::transformer::ReplaceGlobalDefinesConfig;
use oxc_index::IndexVec;
use rolldown_common::side_effects::{DeterminedSideEffects, HookSideEffects};
use rolldown_common::{
  EcmaRelated, ExternalModule, ImportRecordIdx, Module, ModuleDefFormat, ModuleIdx,
  ModuleLoaderMsg, ModuleTable, ModuleType, NormalModuleTaskResult, ResolvedId, SymbolRefDb,
};
use rolldown_error::{BuildDiagnostic, BuildResult};
use rolldown_fs::OsFileSystem;
use rolldown_plugin::SharedPluginDriver;
use rolldown_utils::ecmascript::legitimize_identifier_name;
use rustc_hash::FxHashMap;
use std::sync::Arc;

use crate::{SharedOptions, SharedResolver};

pub struct HmrIntermediateNormalModules {
  pub modules: IndexVec<ModuleIdx, Option<Module>>,
  pub index_ecma_ast: IndexEcmaAst,
}

impl HmrIntermediateNormalModules {
  pub fn new(previous_module_table: ModuleTable, index_ecma_ast: IndexEcmaAst) -> Self {
    Self {
      modules: previous_module_table.modules.into_iter().map(Some).collect::<IndexVec<_, _>>(),
      index_ecma_ast,
    }
  }

  pub fn alloc_ecma_module_idx(&mut self) -> ModuleIdx {
    self.modules.push(None)
  }
}

pub struct HmrModuleLoader {
  options: SharedOptions,
  shared_context: Arc<TaskContext>,
  rx: tokio::sync::mpsc::Receiver<ModuleLoaderMsg>,
  visited: FxHashMap<ArcStr, ModuleIdx>,
  remaining: u32,
  intermediate_normal_modules: HmrIntermediateNormalModules,
  symbol_ref_db: SymbolRefDb,
}

pub struct HmrModuleLoaderOutput {
  // Stored all modules
  pub module_table: ModuleTable,
  pub module_id_to_modules: FxHashMap<ArcStr, ModuleIdx>,
  pub index_ecma_ast: IndexEcmaAst,
  pub symbol_ref_db: SymbolRefDb,
  pub warnings: Vec<BuildDiagnostic>,
  pub changed_modules: Vec<ModuleIdx>,
  pub diff_modules: Vec<ModuleIdx>,
}

impl HmrModuleLoader {
  #[allow(clippy::too_many_arguments)]
  pub fn new(
    options: SharedOptions,
    plugin_driver: SharedPluginDriver,
    fs: OsFileSystem,
    resolver: SharedResolver,
    previous_module_id_to_modules: FxHashMap<ArcStr, ModuleIdx>,
    previous_module_table: ModuleTable,
    pervious_index_ecma_ast: IndexEcmaAst,
    pervious_symbols: SymbolRefDb,
  ) -> anyhow::Result<Self> {
    // 1024 should be enough for most cases
    // over 1024 pending tasks are insane
    let (tx, rx) = tokio::sync::mpsc::channel::<ModuleLoaderMsg>(1024);

    let meta = TaskContextMeta {
      replace_global_define_config: if options.define.is_empty() {
        None
      } else {
        Some(ReplaceGlobalDefinesConfig::new(&options.define).map_err(|errs| {
          // TODO: maybe we should give better diagnostics here. since oxc return
          // `Vec<OxcDiagnostic>`
          anyhow::format_err!(
            "Failed to generate defines config from {:?}. Got {:#?}",
            options.define,
            errs
          )
        })?)
      },
    };
    let common_data = Arc::new(TaskContext {
      options: Arc::clone(&options),
      tx,
      resolver,
      fs,
      plugin_driver,
      meta,
    });

    let intermediate_normal_modules =
      HmrIntermediateNormalModules::new(previous_module_table, pervious_index_ecma_ast);

    Ok(Self {
      shared_context: common_data,
      rx,
      options,
      visited: previous_module_id_to_modules,
      remaining: 0,
      intermediate_normal_modules,
      symbol_ref_db: pervious_symbols,
    })
  }

  fn try_spawn_new_task(
    &mut self,
    resolved_id: ResolvedId,
    owner: Option<ModuleTaskOwner>,
    is_user_defined_entry: bool,
    assert_module_type: Option<ModuleType>,
  ) -> ModuleIdx {
    match self.visited.entry(resolved_id.id.clone()) {
      std::collections::hash_map::Entry::Occupied(visited) => *visited.get(),
      std::collections::hash_map::Entry::Vacant(not_visited) => {
        if resolved_id.is_external {
          let idx = self.intermediate_normal_modules.alloc_ecma_module_idx();
          not_visited.insert(idx);
          let external_module_side_effects = if let Some(hook_side_effects) =
            resolved_id.side_effects
          {
            match hook_side_effects {
              HookSideEffects::True => DeterminedSideEffects::UserDefined(true),
              HookSideEffects::False => DeterminedSideEffects::UserDefined(false),
              HookSideEffects::NoTreeshake => DeterminedSideEffects::NoTreeshake,
            }
          } else {
            match self.options.treeshake {
              rolldown_common::TreeshakeOptions::Boolean(false) => {
                DeterminedSideEffects::NoTreeshake
              }
              rolldown_common::TreeshakeOptions::Boolean(true) => unreachable!(),
              rolldown_common::TreeshakeOptions::Option(ref opt) => match opt.module_side_effects {
                rolldown_common::ModuleSideEffects::Boolean(false) => {
                  DeterminedSideEffects::UserDefined(false)
                }
                _ => DeterminedSideEffects::NoTreeshake,
              },
            }
          };
          let symbol_ref = self.symbol_ref_db.create_facade_root_symbol_ref(
            idx,
            legitimize_identifier_name(resolved_id.id.as_str()).into(),
          );
          let ext = ExternalModule::new(
            idx,
            ArcStr::clone(&resolved_id.id),
            external_module_side_effects,
            symbol_ref,
          );
          self.intermediate_normal_modules.modules[idx] = Some(ext.into());
          idx
        } else {
          let idx = self.intermediate_normal_modules.alloc_ecma_module_idx();
          not_visited.insert(idx);
          self.remaining += 1;

          let task = ModuleTask::new(
            Arc::clone(&self.shared_context),
            idx,
            resolved_id,
            owner,
            is_user_defined_entry,
            assert_module_type,
          );
          #[cfg(target_family = "wasm")]
          {
            let handle = tokio::runtime::Handle::current();
            // could not block_on/spawn the main thread in WASI
            std::thread::spawn(move || {
              handle.spawn(task.run());
            });
          }
          #[cfg(not(target_family = "wasm"))]
          tokio::spawn(task.run());
          idx
        }
      }
    }
  }

  #[tracing::instrument(level = "debug", skip_all)]
  pub async fn fetch_changed_files(
    mut self,
    changed_files: Vec<String>,
  ) -> BuildResult<HmrModuleLoaderOutput> {
    let changed_modules: Vec<ModuleIdx> =
      changed_files.iter().filter_map(|m| self.visited.get(m.as_str())).copied().collect();
    let mut diff_modules: Vec<ModuleIdx> = vec![];
    // spawn valid changed modules
    changed_files
      .into_iter()
      .filter_map(|m| self.visited.get(m.as_str()).map(|idx| (m, idx)))
      .for_each(|(m, idx)| {
        self.remaining += 1;

        let task = ModuleTask::new(
          Arc::clone(&self.shared_context),
          *idx,
          ResolvedId {
            id: m.into(),
            ignored: false,
            module_def_format: ModuleDefFormat::Unknown,
            is_external: false,
            package_json: None,
            side_effects: None,
            is_external_without_side_effects: false,
          },
          None,
          false,
          None,
        );
        #[cfg(target_family = "wasm")]
        {
          let handle = tokio::runtime::Handle::current();
          // could not block_on/spawn the main thread in WASI
          std::thread::spawn(move || {
            handle.spawn(task.run());
          });
        }
        #[cfg(not(target_family = "wasm"))]
        tokio::spawn(task.run());
      });

    let mut errors = vec![];
    let mut all_warnings: Vec<BuildDiagnostic> = vec![];

    while self.remaining > 0 {
      let Some(msg) = self.rx.recv().await else {
        break;
      };
      match msg {
        ModuleLoaderMsg::NormalModuleDone(task_result) => {
          let NormalModuleTaskResult {
            module_idx,
            resolved_deps,
            mut module,
            raw_import_records,
            warnings,
            ecma_related,
          } = task_result;
          all_warnings.extend(warnings);

          let import_records: IndexVec<ImportRecordIdx, rolldown_common::ResolvedImportRecord> =
            raw_import_records
              .into_iter()
              .zip(resolved_deps)
              .map(|(raw_rec, info)| {
                let ecma_module = module.as_normal().unwrap();
                let owner = ModuleTaskOwner::new(
                  ecma_module.source.clone(),
                  ecma_module.stable_id.as_str().into(),
                  raw_rec.span,
                );
                let id = self.try_spawn_new_task(
                  info,
                  Some(owner),
                  false,
                  raw_rec.asserted_module_type.clone(),
                );
                raw_rec.into_resolved(id)
              })
              .collect::<IndexVec<ImportRecordIdx, _>>();

          module.set_import_records(import_records);
          if let Some(EcmaRelated { ast, symbols, .. }) = ecma_related {
            if module_idx < self.intermediate_normal_modules.modules.len() {
              if let Some(ast_idx) = self.intermediate_normal_modules.modules[module_idx]
                .as_ref()
                .and_then(|m| m.as_normal())
                .and_then(|m| m.ecma_ast_idx)
              {
                self.intermediate_normal_modules.index_ecma_ast[ast_idx] = (ast, module_idx);
                module.set_ecma_ast_idx(ast_idx);
              }
            } else {
              let ast_idx = self.intermediate_normal_modules.index_ecma_ast.push((ast, module_idx));
              module.set_ecma_ast_idx(ast_idx);
            }
            self.symbol_ref_db.store_local_db(module_idx, symbols);
          }
          self.intermediate_normal_modules.modules[module_idx] = Some(module);
          diff_modules.push(module_idx);
        }
        ModuleLoaderMsg::FetchModule(resolve_id) => {
          self.try_spawn_new_task(resolve_id, None, false, None);
        }
        ModuleLoaderMsg::RuntimeNormalModuleDone(_) => {
          unreachable!("Runtime module should not be done at hmr module loader");
        }
        ModuleLoaderMsg::BuildErrors(e) => {
          errors.extend(e);
        }
      }
      self.remaining -= 1;
    }

    if !errors.is_empty() {
      return Err(errors.into());
    }

    let modules: IndexVec<ModuleIdx, Module> =
      self.intermediate_normal_modules.modules.into_iter().flatten().collect();

    Ok(HmrModuleLoaderOutput {
      module_table: ModuleTable { modules },
      module_id_to_modules: self.visited,
      symbol_ref_db: self.symbol_ref_db,
      index_ecma_ast: self.intermediate_normal_modules.index_ecma_ast,
      warnings: all_warnings,
      changed_modules,
      diff_modules,
    })
  }
}
