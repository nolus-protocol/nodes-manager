import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import { resolve } from 'path'

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [vue()],
  root: 'ui',
  build: {
    outDir: '../dist-ui',
    emptyOutDir: true,
    rollupOptions: {
      external: [],
      output: {
        manualChunks: undefined,
      }
    },
    commonjsOptions: {
      include: [/node_modules/],
      transformMixedEsModules: true,
    }
  },
  server: {
    port: 5173,
    proxy: {
      // Proxy API requests to the Rust backend
      '/api': {
        target: 'http://localhost:3000',
        changeOrigin: true,
      }
    }
  },
  resolve: {
    alias: {
      '@': resolve(__dirname, './ui/src'),
    }
  },
  optimizeDeps: {
    include: ['vue', 'web-components'],
    exclude: [],
    esbuildOptions: {
      target: 'esnext'
    }
  }
})
