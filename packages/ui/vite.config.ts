import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vite';
import vue from '@vitejs/plugin-vue';

const root = fileURLToPath(new URL('.', import.meta.url));

// Vite 配置面向 Tauri（devUrl=localhost:1420）。
// build 禁用 CSS 拆包：关键 CSS 内联进 html，规避首帧 FOUC（§4.6 A.8）。
export default defineConfig({
  plugins: [vue()],
  clearScreen: false,
  envPrefix: ['VITE_', 'TAURI_'],
  root,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      ignored: ['**/packages/native/**', '**/target/**'],
    },
  },
  build: {
    target: ['es2021', 'safari15'],
    cssCodeSplit: false,
    sourcemap: false,
    outDir: 'dist',
    rollupOptions: {
      input: {
        main: fileURLToPath(new URL('./index.html', import.meta.url)),
        settings: fileURLToPath(new URL('./settings.html', import.meta.url)),
        study: fileURLToPath(new URL('./study.html', import.meta.url)),
      },
    },
  },
});
