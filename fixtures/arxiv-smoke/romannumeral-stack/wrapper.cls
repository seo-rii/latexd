\ProvidesClass{wrapper}[2024/01/01]
\def\wrapperclass{wrapper}
\PassOptionsToPackage{unicode}{shim}
\def\wrappermode{wrapper-\romannumeral 1}
\DeclareOption{twocolumn}{\def\wrappermode{wrapper-\romannumeral 2}}
\ProcessOptions\relax
\LoadClassWithOptions{article}
\RequirePackageWithOptions{shim}
