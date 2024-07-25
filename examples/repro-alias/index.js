import * as rolldown from "rolldown";
import path from "node:path";

async function main() {
  const bundle = await rolldown.rolldown({
    cwd: import.meta.dirname,
    // works if single entry (each one works alone)
    input: [
      "./src/main.js",
      "foo",
    ],
    resolve: {
      alias: {
        "foo": path.join(import.meta.dirname, "./src/main.js")
      }
    }
  });
  await bundle.write({
    format: "esm",
  });
  process.exit(0);
}

main();
