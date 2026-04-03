import {defineConfig} from 'vite'
import react from '@vitejs/plugin-react'

// https://vite.dev/config/
export default defineConfig({
    server: {
        proxy: {
            '/api': 'http://localhost:8080/',
            '/ws': {target: 'ws://localhost:8080', ws: true},
        },
    },
    plugins: [react()],
})
