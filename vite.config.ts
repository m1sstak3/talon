import { defineConfig } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'

// ponytail: fixed port, Tauri dev will point at it later
export default defineConfig({
  plugins: [svelte()],
  server: { port: 1420, strictPort: true, watch: { ignored: ['**/src-tauri/**'] } },
  clearScreen: false,
  build: { target: 'es2022' },
})
