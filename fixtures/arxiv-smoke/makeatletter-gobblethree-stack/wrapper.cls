\ProvidesClass{wrapper}[2024/01/01]
\def\wrapperclass{wrapper}
\PassOptionsToPackage{unicode}{shim}
\makeatletter
\def\wrappertail{\@gobblethree{drop-a}{drop-b}{drop-c}wrapper-tail}
\makeatother
\LoadClassWithOptions{article}
\RequirePackageWithOptions{shim}
