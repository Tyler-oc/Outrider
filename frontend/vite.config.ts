import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// https://vite.dev/config/
export default defineConfig({
  plugins: [react()],
  server: {
    proxy: {
      '/campgrounds': 'http://localhost:3001',
      '/search': 'http://localhost:3001',
    },
  },
})
