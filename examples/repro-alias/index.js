import * as rolldown from "rolldown";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);

async function main() {
  const bundle = await rolldown.rolldown({
    cwd: import.meta.dirname,
    // works if single entry (each one works alone)
    input: [
      "test-dep-main",
      "foo",
    ],
    resolve: {
      alias: {
        "foo": "test-dep-main",
      },
    }
  });
  await bundle.write({
    format: "esm",
  });
  process.exit(0);
}

main();
