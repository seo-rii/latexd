\ProvidesClass{wrapper}[2024/01/01]
\def\wrapperclass{wrapper}
\DeclareOption{brokenclass}{\ClassError{wrapper}{class failure}{remove brokenclass}}
\ProcessOptions\relax
\LoadClassWithOptions{article}
