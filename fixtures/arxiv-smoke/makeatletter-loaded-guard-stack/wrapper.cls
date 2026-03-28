\ProvidesClass{wrapper}[2024/01/01]
\def\wrapperclass{wrapper}
\PassOptionsToPackage{unicode}{shim}
\LoadClassWithOptions{article}
\makeatletter
\@ifclassloaded{article}{\def\classguard{class-loaded}}{\def\classguard{class-missing}}
\makeatother
\RequirePackageWithOptions{shim}
