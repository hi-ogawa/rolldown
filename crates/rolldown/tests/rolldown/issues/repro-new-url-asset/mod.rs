use std::{borrow::Cow, sync::Arc};

use rolldown::{BundlerOptions, InputItem};
use rolldown_plugin::{Plugin, SharedPluginContext};
use rolldown_testing::{abs_file_dir, integration_test::IntegrationTest, test_config::TestMeta};

// cargo test -p rolldown --test integration_rolldown repro_new_url_asset

#[derive(Debug)]
struct AssetImportMetaUrlPlugin {}

// https://github.com/vitejs/vite/blob/b3f5dfef8da92197e0d8eec0507f2c6ef7467418/packages/vite/src/node/plugins/assetImportMetaUrl.ts#L26

impl Plugin for AssetImportMetaUrlPlugin {
  fn name(&self) -> Cow<'static, str> {
    "asset-import-meta-url".into()
  }

  fn transform_ast(
    &self,
    _ctx: &SharedPluginContext,
    args: rolldown_plugin::HookTransformAstArgs,
  ) -> rolldown_plugin::HookTransformAstReturn {
    Ok(args.ast)
  }
}

#[tokio::test(flavor = "multi_thread")]
async fn should_rewrite_dynamic_imports_that_import_external_modules() {
  let cwd = abs_file_dir!();

  IntegrationTest::new(TestMeta { expect_executed: false, ..Default::default() })
    .run_with_plugins(
      BundlerOptions {
        input: Some(vec![InputItem {
          name: Some("entry".to_string()),
          import: "./entry.js".to_string(),
        }]),
        cwd: Some(cwd),
        ..Default::default()
      },
      vec![Arc::new(AssetImportMetaUrlPlugin {})],
    )
    .await;
}
