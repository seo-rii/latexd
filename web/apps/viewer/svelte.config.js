import adapter from '@sveltejs/adapter-static';
import { relative, sep } from 'node:path';
import { normalizeBasePath } from './base-path.mjs';

const viewerBasePath = normalizeBasePath(process.env.LATEXD_VIEWER_BASE_PATH);

/** @type {import('@sveltejs/kit').Config} */
const config = {
	compilerOptions: {
		// defaults to rune mode for the project, execept for `node_modules`. Can be removed in svelte 6.
		runes: ({ filename }) => {
			const relativePath = relative(import.meta.dirname, filename);
			const pathSegments = relativePath.toLowerCase().split(sep);
			const isExternalLibrary = pathSegments.includes('node_modules');

			return isExternalLibrary ? undefined : true;
		}
	},
	kit: {
		adapter: adapter({
			pages: 'build',
			assets: 'build'
		}),
		prerender: {
			entries: ['/']
		},
		paths: {
			base: viewerBasePath
		}
	}
};

export default config;
