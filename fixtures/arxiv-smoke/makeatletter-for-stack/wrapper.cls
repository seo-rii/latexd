\ProvidesClass{wrapper}[2024/01/01]
\def\wrapperclass{wrapper}
\PassOptionsToPackage{unicode}{shim}
\makeatletter
\def\wrapperfor{\@for\wrapperitem:=wrap,{per}\do{\wrapperitem}}
\makeatother
\LoadClassWithOptions{article}
\RequirePackageWithOptions{shim}
