import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig, loadEnv } from 'vite';

export default defineConfig(({ mode }) => {
	const env = loadEnv(mode, '.', '');
	const latexdDevOrigin = env.LATEXD_DEV_ORIGIN || 'http://127.0.0.1:4380';

	return {
		plugins: [sveltekit()],
		server: {
			proxy: {
				'/api': latexdDevOrigin,
				'/artifacts': latexdDevOrigin,
				'/ws': {
					target: latexdDevOrigin,
					ws: true
				}
			}
		}
	};
});
