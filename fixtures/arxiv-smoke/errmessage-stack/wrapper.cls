\NeedsTeXFormat{LaTeX2e}
\ProvidesClass{wrapper}[2024/01/01 Wrapper class fixture]
\DeclareOption{broken}{\errmessage{class failure}}
\PassOptionsToPackage{unicode}{shim}
\LoadClassWithOptions{article}
\ProcessOptions\relax
