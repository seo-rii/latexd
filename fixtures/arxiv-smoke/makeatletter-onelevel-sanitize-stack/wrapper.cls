\ProvidesClass{wrapper}[2024/01/01]
\def\wrapperclass{wrapper}
\PassOptionsToPackage{unicode}{shim}
\def\wrapperhook{wrapper defined}
\makeatletter
\@onelevel@sanitize\wrapperhook
\makeatother
\LoadClassWithOptions{article}
\RequirePackageWithOptions{shim}
