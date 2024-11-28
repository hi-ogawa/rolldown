import { defineTest } from '@tests'
import { join } from 'node:path'
import { expect } from 'vitest'

export default defineTest({
  config: {
    plugins: [
      {
        name: 'my-plugin',
        async transform() {
          return {
            code: 'export {}',
            map: {
              mappings: '@invalid'
            }
          }
        },
      },
    ],
  },
  catchError(e: any) {
    console.log(e)
    // expect(e.message).toContain('[plugin my-plugin]')
    // expect(e.message).toContain('my-error')
    // expect(e.message).toContain('at errorFn2')
    // expect(e.message).toContain('at errorFn1')
    // expect(e.errors[0]).toMatchObject({
    //   message: 'my-error',
    //   extraProp: 1234,
    //   code: 'PLUGIN_ERROR',
    //   plugin: 'my-plugin',
    //   hook: 'transform',
    //   id: join(import.meta.dirname, './main.js'),
    // })
  },
})
