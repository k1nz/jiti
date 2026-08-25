import { defineConfig } from 'vite';
import vue from '@vitejs/plugin-vue';

// Vite 配置面向 Tauri（devUrl=localhost:1420）。
// build 禁用 CSS 拆包：关键 CSS 内联进 index.html，规避首帧 FOUC（§4.6 A.8）。
export default defineConfig({
  plugins: [vue()],
  clearScreen: false,
  envPrefix: ['VITE_', 'TAURI_'],
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
    outDir: 'dist',
  },
});