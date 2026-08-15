use tex_render_model::{CaptionKind, FootnoteCommandKind, MetadataField, PageBreakKind};
use tex_tokens::Token;

use crate::snapshot::{IntegerParameterId, LayoutIntegerParameterId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IntegerParameterCommand {
    Tolerance(IntegerParameterId),
    Layout(LayoutIntegerParameterId),
}

impl IntegerParameterCommand {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Tolerance(parameter) => parameter.as_str(),
            Self::Layout(parameter) => parameter.as_str(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct MacroFlags {
    pub(crate) long: bool,
    pub(crate) outer: bool,
    pub(crate) protected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Meaning {
    Macro(MacroDefinition),
    Primitive(Primitive),
    Token(Token),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MacroDefinition {
    pub(crate) flags: MacroFlags,
    pub(crate) parameter_count: u8,
    pub(crate) parameter_text: Vec<Token>,
    pub(crate) optional_first_argument_default: Option<Vec<Token>>,
    pub(crate) body: Vec<Token>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReferenceCommand {
    pub(crate) canonical_name: &'static str,
    pub(crate) key_argument_count: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LinkCommand {
    pub(crate) canonical_name: &'static str,
    pub(crate) has_separate_text_argument: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HeadingCommand {
    pub(crate) canonical_name: &'static str,
    pub(crate) level: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CaptionCommand {
    pub(crate) canonical_name: &'static str,
    pub(crate) fixed_kind: Option<CaptionKind>,
    pub(crate) reads_kind_argument: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GraphicCommand {
    pub(crate) canonical_name: &'static str,
    pub(crate) include_pdf: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LegacyGraphicSyntax {
    KeyValue,
    File,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MathDelimiterCommand {
    InlineOpen,
    InlineClose,
    DisplayOpen,
    DisplayClose,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LegacyGraphicCommand {
    pub(crate) canonical_name: &'static str,
    pub(crate) syntax: LegacyGraphicSyntax,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EpsfDimension {
    Width,
    Height,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BibliographyMetadataCommand {
    AddResource,
    Style,
    UrlStyle,
    NoCite,
    DefineAlias,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BibliographyTextCommand {
    pub(crate) canonical_name: &'static str,
    pub(crate) visible_text: &'static str,
    pub(crate) attach_next: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TextSymbolCommand {
    pub(crate) canonical_name: &'static str,
    pub(crate) visible_text: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TextScriptCommand {
    Superscript,
    Subscript,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BibliographyWrapperCommand {
    pub(crate) canonical_name: &'static str,
    pub(crate) prefix: &'static str,
    pub(crate) suffix: &'static str,
    pub(crate) separate_before: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BibliographyFieldCommand {
    pub(crate) canonical_name: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NatbibSplitSuffixCommand {
    pub(crate) canonical_name: &'static str,
    pub(crate) source_suffix: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PhantomWrapperCommand {
    pub(crate) canonical_name: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BoxWrapperCommand {
    FrameBox,
    MakeBox,
    RaiseBox,
    ParBox,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Primitive {
    Relax,
    Par,
    LineBreak,
    PageBreak(PageBreakKind),
    Footnote(FootnoteCommandKind),
    FootnoteMark,
    DocumentMetadata(MetadataField),
    FlushTitleBlock,
    IcmlAuthor,
    IcmlAffiliation,
    IcmlCorrespondingAuthor,
    IcmlFlushTitleBlock,
    MathDelimiter(MathDelimiterCommand),
    EnsureMath,
    LegacyMathWordBoundary,
    LegacyTextScriptBoundary,
    Def,
    GlobalDef,
    ExpandedDef,
    GlobalExpandedDef,
    Let,
    FutureLet,
    String,
    Immediate,
    Protect,
    NewRead,
    OpenIn,
    CloseIn,
    Read,
    ReadLine,
    NewWrite,
    OpenOut,
    CloseOut,
    Write,
    ProtectedWrite,
    IgnoreSpaces,
    LeaveMode,
    Unskip,
    JobName,
    CurrentModuleName,
    CurrentModuleExt,
    CurrentModulePath,
    FilenameParse,
    Meaning,
    Detokenize,
    StripPrefix,
    TextWrapper,
    Uppercase,
    Lowercase,
    ExpandAfter,
    ExpandedTokens,
    Unexpanded,
    CsName,
    EndCsName,
    ExcludeComment,
    IncludeComment,
    BeginEnvironment,
    EndEnvironment,
    Item,
    BibliographyItem,
    Bibliography,
    PrintBibliography,
    BibliographyMetadata(BibliographyMetadataCommand),
    BibliographyField(BibliographyFieldCommand),
    BibliographyString,
    BibliographyText(BibliographyTextCommand),
    TextSymbol(TextSymbolCommand),
    TextScript(TextScriptCommand),
    BibliographyWrapper(BibliographyWrapperCommand),
    NatbibSplitSuffix(NatbibSplitSuffixCommand),
    PhantomWrapper(PhantomWrapperCommand),
    Rule,
    BoxWrapper(BoxWrapperCommand),
    BeginGroupCommand,
    EndGroupCommand,
    AfterGroup,
    AfterAssignment,
    Long,
    Protected,
    Outer,
    Global,
    Unless,
    MakeAtLetter,
    MakeAtOther,
    IfTrue,
    IfFalse,
    NewIf,
    CharDef,
    CatCode,
    MathCode,
    DelCode,
    IntegerParameter(IntegerParameterCommand),
    NeedsTeXFormat,
    ProvidesFile,
    ProvidesPackage,
    ProvidesClass,
    DocumentClass,
    LoadClass,
    LoadClassWithOptions,
    RequirePackage,
    UsePackage,
    RequirePackageWithOptions,
    PassOptionsToClass,
    PassOptionsToPackage,
    DeclareOption,
    ExecuteOptions,
    ProcessOptions,
    AtBeginDocument,
    AtEndDocument,
    AtEndOfPackage,
    AtEndOfClass,
    Message,
    Typeout,
    WriteLog,
    NewCount,
    CountDef,
    NewDimen,
    DimenDef,
    NewSkip,
    SkipDef,
    NewMuSkip,
    MuSkipDef,
    SetCounter,
    AddToCounter,
    StepCounter,
    RefStepCounter,
    AddToReset,
    RemoveFromReset,
    NewLength,
    SetLength,
    AddToLength,
    NewToks,
    ToksDef,
    NewCommand,
    DeclareRobustCommand,
    RenewCommand,
    ProvideCommand,
    DeclareMathOperator,
    PackageInfo,
    PackageInfoNoLine,
    ClassInfo,
    ClassInfoNoLine,
    PackageWarning,
    PackageWarningNoLine,
    ClassWarning,
    ClassWarningNoLine,
    PackageError,
    ClassError,
    GenericInfo,
    GenericWarning,
    GenericError,
    ErrMessage,
    LatexInfo,
    LatexWarning,
    LatexWarningNoLine,
    LatexError,
    IfPackageLoadedTF,
    IfClassLoadedTF,
    IfPackageAtLeastTF,
    IfClassAtLeastTF,
    IfPackageLater,
    IfClassLater,
    IfPackageWith,
    IfClassWith,
    IfEof,
    IfFileExists,
    InputIfFileExists,
    AtInput,
    OnlyPreamble,
    OneLevelSanitize,
    BspHack,
    Esphack,
    InAt,
    IfInAt,
    TempSwaTrue,
    TempSwaFalse,
    IfTempSwa,
    FileswTrue,
    FileswFalse,
    IfFilesw,
    NoFiles,
    Loop,
    For,
    WhileNum,
    WhileSw,
    IfCsName,
    IfDefined,
    IfUndefined,
    IfDefinable,
    IfNextChar,
    IfStar,
    IfEmpty,
    IfNotEmpty,
    IfMtArg,
    IfNotMtArg,
    TestOpt,
    DblArg,
    Car,
    Cdr,
    TFor,
    Cons,
    RemoveElement,
    ThirdOfThree,
    ExpandTwoArgs,
    ZapSpace,
    FirstOfOne,
    Iden,
    FirstOfTwo,
    SecondOfTwo,
    Gobble,
    GobbleTwo,
    GobbleThree,
    GobbleFour,
    GAddToMacro,
    NameDef,
    NameXDef,
    NameUse,
    Advance,
    Multiply,
    Divide,
    IfChar,
    IfOdd,
    IfCat,
    IfCase,
    IfX,
    IfNum,
    IfDim,
    Else,
    Fi,
    Count,
    Dimen,
    Skip,
    MuSkip,
    Toks,
    Value,
    The,
    Number,
    RomanNumeral,
    CounterArabic,
    CounterRoman,
    CounterRomanUpper,
    CounterAlph,
    CounterAlphUpper,
    EndInput,
    Input,
    Include,
    IncludeOnly,
    Label,
    Citation,
    Reference(ReferenceCommand),
    Link(LinkCommand),
    Heading(HeadingCommand),
    Caption(CaptionCommand),
    Graphic(GraphicCommand),
    LegacyGraphic(LegacyGraphicCommand),
    EpsfDimension(EpsfDimension),
    GraphicPath,
    DeclareGraphicsExtensions,
    SetKeys,
}
