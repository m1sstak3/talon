import { defineConfig } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'

// `tauri android dev` sets TAURI_DEV_HOST: the phone or emulator loads the page and hot updates from this machine
const host = process.env.TAURI_DEV_HOST
export default defineConfig({
  plugins: [svelte()],
  server: { port: 1420, strictPort: true, host: host || false, hmr: host ? { protocol: 'ws', host } : undefined, watch: { ignored: ['**/src-tauri/**'] } },
  clearScreen: false,
  build: { target: 'es2022' },
})
