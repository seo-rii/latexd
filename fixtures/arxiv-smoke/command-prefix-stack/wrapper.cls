\ProvidesClass{wrapper}[2024/01/01]
\def\wrapperclass{wrapper}
\PassOptionsToPackage{unicode}{shim}
{\global\long\def\wrapperprefix#1{<#1>}\global\let\wrapperalias\wrapperprefix\global\protected\def\wrapperflag{wrapper-one}\global\outer\def\wrapperready{ready}\global\newcount\wrappercount\wrappercount=1}
\DeclareOption{twocolumn}{{\global\protected\def\wrapperflag{wrapper-two}\advance\wrappercount by 1}}
\ProcessOptions\relax
\LoadClassWithOptions{article}
\RequirePackageWithOptions{shim}
