import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig, loadEnv, type ProxyOptions } from 'vite';
import { normalizeBasePath } from './base-path.mjs';

export default defineConfig(({ mode }) => {
	const env = loadEnv(mode, '.', '');
	const latexdDevOrigin = env.LATEXD_DEV_ORIGIN || 'http://127.0.0.1:4380';
	const viewerBasePath = normalizeBasePath(env.LATEXD_VIEWER_BASE_PATH);
	const proxy: Record<string, string | ProxyOptions> = {
		'/api': latexdDevOrigin,
		'/artifacts': latexdDevOrigin,
		'/ws': {
			target: latexdDevOrigin,
			ws: true
		}
	};

	if (viewerBasePath) {
		proxy[`${viewerBasePath}/api`] = latexdDevOrigin;
		proxy[`${viewerBasePath}/artifacts`] = latexdDevOrigin;
		proxy[`${viewerBasePath}/ws`] = {
			target: latexdDevOrigin,
			ws: true
		};
	}

	return {
		plugins: [sveltekit()],
		server: {
			proxy,
			allowedHosts: true
		}
	};
});
