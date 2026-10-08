import { defineConfig } from 'vite';

export default defineConfig({
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    // O Tauri recompila o Rust sozinho; vigiar src-tauri/target trava o Vite com arquivos em uso (EBUSY).
    // Função em vez de glob: o glob não casava com os caminhos com "\" do Windows.
    watch: { ignored: (path) => /[\\/]src-tauri([\\/]|$)/.test(path) },
  },
  build: { target: 'chrome105', outDir: 'dist' },
});
