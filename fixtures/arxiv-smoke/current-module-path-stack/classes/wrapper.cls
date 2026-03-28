\NeedsTeXFormat{LaTeX2e}
\ProvidesClass{wrapper}[2024/01/01 Wrapper class fixture]
\makeatletter\edef\wrapperpath{\@currpath}\makeatother
\LoadClassWithOptions{article}
\PassOptionsToPackage{unicode}{cfgs/shim}
\RequirePackageWithOptions{cfgs/shim}
