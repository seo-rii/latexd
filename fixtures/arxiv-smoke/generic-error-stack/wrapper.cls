\NeedsTeXFormat{LaTeX2e}
\ProvidesClass{wrapper}[2024/01/01 Wrapper class fixture]
\DeclareOption{broken}{\GenericError{wrapper}{class failure}{remove broken}{continue}}
\PassOptionsToPackage{unicode}{shim}
\LoadClassWithOptions{article}
\ProcessOptions\relax
