\ProvidesClass{wrapper}[2024/01/01]
\def\wrapperclass{wrapper}
\PassOptionsToPackage{unicode}{shim}
\makeatletter
\def\wrappertfor{\@tfor\wrapperitem:=wr{ap}\do{\wrapperitem}}
\makeatother
\LoadClassWithOptions{article}
\RequirePackageWithOptions{shim}
