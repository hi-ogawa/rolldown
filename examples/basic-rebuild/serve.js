// @ts-check
import { defineConfig } from 'rolldown'
import { rebuild } from 'rolldown/experimental'
import fs from 'node:fs'

const config = defineConfig({
  input: {
    entry: './src/index.js',
  },
  output: {
    dir: './dist',
  },
  plugins: [
    {
      name: 'test',
      renderChunk(code, chunk) {
        console.log('[renderChunk]', chunk, { code })
      },
    },
  ],
})

/**
 * @param {string} filepath
 * @param {(s: string) => string} editFn
 */
function edit(filepath, editFn) {
  fs.writeFileSync(filepath, editFn(fs.readFileSync(filepath, 'utf-8')))
}

async function main() {
  const bundle = await rebuild(config)
  const output1 = await bundle.build()
  console.log(output1.output)
  edit('./src/dep.js', (s) =>
    s.replace(/true|false/, (m) => (m === 'true' ? 'false' : 'true')),
  )
  const output2 = await bundle.build()
  console.log(output2.output)
}

main()
