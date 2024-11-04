import { defineConfig } from 'vite'
import { viteroll } from './node_modules/@hiogawa/viteroll/viteroll.ts'

export default defineConfig({
  clearScreen: false,
  plugins: [
    viteroll({
      reactRefresh: true,
    }),
  ],
})
