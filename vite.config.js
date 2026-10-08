import { defineConfig } from 'vite';

export default defineConfig({
  clearScreen: false,
  // O Tauri recompila o Rust sozinho; vigiar src-tauri/target trava o Vite com arquivos em uso (EBUSY).
  server: { port: 1420, strictPort: true, watch: { ignored: ['**/src-tauri/**'] } },
  build: { target: 'chrome105', outDir: 'dist' },
});
