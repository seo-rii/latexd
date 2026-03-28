\ProvidesClass{wrapper}[2024/01/01]
\def\wrapperclass{wrapper}
\PassOptionsToPackage{unicode}{shim}
\makeatletter
\def\wrappertail{\@gobblefour{drop-a}{drop-b}{drop-c}{drop-d}wrapper-tail}
\makeatother
\LoadClassWithOptions{article}
\RequirePackageWithOptions{shim}
