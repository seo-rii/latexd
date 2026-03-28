\ProvidesClass{wrapper}[2024/01/01]
\def\wrapperclass{wrapper}
\def\wrapperchar{W}
\PassOptionsToPackage{unicode}{shim}
\DeclareOption{twocolumn}{\def\wrapperchar{W}}
\ProcessOptions\relax
\LoadClassWithOptions{article}
\RequirePackageWithOptions{shim}
