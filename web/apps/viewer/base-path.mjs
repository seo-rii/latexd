/**
 * @param {string | undefined | null} value
 */
export function normalizeBasePath(value) {
	if (typeof value !== 'string') {
		return '';
	}

	const trimmed = value.trim();
	if (trimmed === '' || trimmed === '/') {
		return '';
	}

	const withoutTrailingSlash = trimmed.replace(/\/+$/, '');
	return withoutTrailingSlash.startsWith('/') ? withoutTrailingSlash : `/${withoutTrailingSlash}`;
}
