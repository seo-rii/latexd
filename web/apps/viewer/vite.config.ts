import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig, loadEnv, type ProxyOptions } from 'vite';
import { normalizeBasePath } from './base-path.mjs';

export default defineConfig(({ mode }) => {
	const env = loadEnv(mode, '.', '');
	const latexdDevOrigin = env.LATEXD_DEV_ORIGIN || 'http://127.0.0.1:4380';
	const viewerBasePath = normalizeBasePath(env.LATEXD_VIEWER_BASE_PATH);
	// The dev server proxies the daemon API, which can write project files, so it
	// listens on loopback with Vite's Host check unless LAN access is requested.
	const devHost = env.LATEXD_VIEWER_DEV_HOST || undefined;
	const allowedHosts = env.LATEXD_VIEWER_ALLOWED_HOSTS?.split(',')
		.map((host) => host.trim())
		.filter(Boolean);
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
			host: devHost,
			proxy,
			allowedHosts
		}
	};
});
