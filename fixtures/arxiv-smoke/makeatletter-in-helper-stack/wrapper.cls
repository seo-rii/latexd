\ProvidesClass{wrapper}[2024/01/01]
\def\wrapperclass{wrapper}
\PassOptionsToPackage{unicode}{shim}
\makeatletter
\def\wrapperin{\in@{ap}{wrap}\ifin@ wrapper-hit\else wrapper-miss\fi\in@{zzz}{wrap}\ifin@ wrapper-bad\else wrapper-ok\fi}
\makeatother
\LoadClassWithOptions{article}
\RequirePackageWithOptions{shim}
