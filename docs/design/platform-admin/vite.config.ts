import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import { viteSingleFile } from 'vite-plugin-singlefile'
export default defineConfig({
  plugins: [react(), viteSingleFile()],
  build: { rollupOptions: { input: 'board.html' }, assetsInlineLimit: 1e9, chunkSizeWarningLimit: 1e9, cssCodeSplit: false },
})
