\ProvidesClass{wrapper}[2024/02/01]
\def\wrapperclass{wrapper}
\PassOptionsToPackage{unicode}{shim}
\LoadClassWithOptions{article}
\makeatletter
\@ifclasslater{article}{2023/12/31}{\def\classversion{class-later}}{\def\classversion{class-not-later}}
\makeatother
\RequirePackageWithOptions{shim}
