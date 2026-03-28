\ProvidesClass{wrapper}[2024/01/01]
\def\wrappermode{one}
\PassOptionsToPackage{unicode}{shim}
\DeclareOption{twocolumn}{\def\wrappermode{two}}
\AtEndOfClass{\edef\wrappertail{class-\wrappermode}}
\ProcessOptions\relax
\LoadClassWithOptions{article}
