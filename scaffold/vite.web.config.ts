import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { fileURLToPath, URL } from 'url';

/* Float IDE · Nexus OS 26 Web 版构建。
 * 产物直接落在 Nexus OS 26 的 apps/float/ 里，宿主用一个 iframe 装它。
 * base 必须是 './'——iframe 的 src 是相对路径，绝对路径 /assets 会打到宿主域名上。 */
const outDir = fileURLToPath(new URL('../../../../deskbox/files/小方规划/小游戏/nexus os 26/nexus os 26/apps/float', import.meta.url));

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  base: './',
  build: {
    target: 'es2020',
    chunkSizeWarningLimit: 9000,
    emptyOutDir: true,
    outDir
  }
});
