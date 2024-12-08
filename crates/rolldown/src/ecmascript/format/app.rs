use rolldown_sourcemap::SourceJoiner;

use crate::{ecmascript::ecma_generator::RenderedModuleSources, types::generator::GenerateContext};

pub fn render_app<'code>(
  ctx: &GenerateContext<'_>,
  hashbang: Option<&'code str>,
  banner: Option<&'code str>,
  intro: Option<&'code str>,
  outro: Option<&'code str>,
  footer: Option<&'code str>,
  module_sources: &'code RenderedModuleSources,
) -> SourceJoiner<'code> {
  let mut source_joiner = SourceJoiner::default();

  if let Some(hashbang) = hashbang {
    source_joiner.append_source(hashbang);
  }
  if let Some(banner) = banner {
    source_joiner.append_source(banner);
  }

  if let Some(intro) = intro {
    source_joiner.append_source(intro);
  }

  // chunk content
  source_joiner.append_source("var __rolldown_modules = {\n");
  module_sources.iter().for_each(|(_, module_id, module_render_output)| {
    source_joiner.append_source(format!(
      "{}: function(__rolldown_runtime) {{",
      // TODO: can we preserve \0 as js string?
      serde_json::to_string(&module_id.stabilize(&ctx.options.cwd)).unwrap()
    ));
    if let Some(emitted_sources) = module_render_output {
      for source in emitted_sources.as_ref() {
        source_joiner.append_source(source);
      }
    }
    source_joiner.append_source("},\n");
  });
  source_joiner.append_source("};\n");

  if let Some(outro) = outro {
    source_joiner.append_source(outro);
  }

  if let Some(footer) = footer {
    source_joiner.append_source(footer);
  }

  source_joiner
}
