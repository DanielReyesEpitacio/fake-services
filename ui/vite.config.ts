import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import vuetify from 'vite-plugin-vuetify'
import { readFileSync, writeFileSync } from 'node:fs'
import { homedir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath, URL } from 'node:url'

/** En desarrollo, el proxy añade el token de administración local. */
function adminToken(): string {
  const dataDir =
    process.env.FAKES_DATA_DIR ??
    (process.platform === 'win32'
      ? join(process.env.APPDATA ?? homedir(), 'FakeServices', 'fake-services', 'data')
      : join(process.env.XDG_DATA_HOME ?? join(homedir(), '.local', 'share'), 'fake-services'))
  try {
    return readFileSync(join(dataDir, 'admin-token'), 'utf8').trim()
  } catch {
    return ''
  }
}

const adminUrl = process.env.FAKES_ADMIN_URL ?? 'http://127.0.0.1:4010'

export default defineConfig({
  plugins: [
    vue(),
    vuetify({ autoImport: true }),
    {
      // dist/ se embebe en el binario de Rust y debe existir aunque esté vacío.
      name: 'keep-dist-placeholder',
      apply: 'build',
      closeBundle: () => writeFileSync(join('dist', '.gitkeep'), ''),
    },
  ],
  resolve: {
    alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) },
  },
  server: {
    port: 5173,
    proxy: {
      '/api': {
        target: adminUrl,
        changeOrigin: true,
        headers: { Authorization: `Bearer ${adminToken()}` },
      },
    },
  },
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    chunkSizeWarningLimit: 1500,
  },
})
