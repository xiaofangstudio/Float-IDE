import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// 主界面构建。渲染层（vendor.js）由 vite.render.config.ts 先行产出到 public/render，
// 这里的构建会原样把它拷进 dist/render。
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 1430, strictPort: true, host: '127.0.0.1' },
  build: {
    target: 'es2020',
    chunkSizeWarningLimit: 9000,
    emptyOutDir: false,
    outDir: 'dist'
  }
});
