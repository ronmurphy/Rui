/// Generate a GtkSourceView colour scheme XML file and write it to
/// ~/.local/share/gtksourceview-5/styles/rui-theme.xml.
///
/// Called once at startup before GTK initialises.
pub fn generate_rui_scheme() {
    // Catppuccin Mocha defaults — a great dark theme for code.
    let bg      = "#1e1e2e";
    let fg      = "#cdd6f4";
    let surface = "#313244";
    let muted   = "#6c7086";
    let accent  = "#89b4fa";
    let green   = "#a6e3a1";
    let yellow  = "#f9e2af";
    let red     = "#f38ba8";
    let cyan    = "#89dceb";
    let purple  = "#cba6f7";
    let subtle  = "#bac2de";

    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<style-scheme id="rui-theme" name="Rui Theme" version="1.0">
  <author>Rui (generated)</author>
  <description>Default colour scheme for the Rui UI designer.</description>

  <!-- Palette colours -->
  <color name="bg"      value="{bg}"/>
  <color name="fg"      value="{fg}"/>
  <color name="surface" value="{surface}"/>
  <color name="muted"   value="{muted}"/>
  <color name="accent"  value="{accent}"/>
  <color name="green"   value="{green}"/>
  <color name="yellow"  value="{yellow}"/>
  <color name="red"     value="{red}"/>
  <color name="cyan"    value="{cyan}"/>
  <color name="purple"  value="{purple}"/>
  <color name="subtle"  value="{subtle}"/>

  <!-- Base styles -->
  <style name="text"                  foreground="fg"     background="bg"/>
  <style name="selection"             background="accent" foreground="bg"/>
  <style name="current-line"          background="surface"/>
  <style name="line-numbers"          foreground="muted"  background="bg"/>
  <style name="right-margin"          foreground="surface"/>
  <style name="bracket-match"         foreground="cyan"   bold="true"/>
  <style name="bracket-mismatch"      foreground="red"    bold="true"/>
  <style name="search-match"          background="yellow" foreground="bg"/>

  <!-- Syntax token styles -->
  <style name="def:comment"           foreground="muted"  italic="true"/>
  <style name="def:doc-comment"       foreground="muted"  italic="true"/>
  <style name="def:string"            foreground="green"/>
  <style name="def:special-char"      foreground="cyan"/>
  <style name="def:keyword"           foreground="accent" bold="true"/>
  <style name="def:builtin"           foreground="cyan"/>
  <style name="def:type"              foreground="purple"/>
  <style name="def:class"             foreground="purple"/>
  <style name="def:function"          foreground="accent"/>
  <style name="def:constant"          foreground="yellow"/>
  <style name="def:number"            foreground="yellow"/>
  <style name="def:base-n-integer"    foreground="yellow"/>
  <style name="def:floating-point"    foreground="yellow"/>
  <style name="def:boolean"           foreground="yellow"/>
  <style name="def:preprocessor"      foreground="cyan"   italic="true"/>
  <style name="def:error"             foreground="red"    underline="true"/>
  <style name="def:warning"           foreground="yellow" underline="true"/>
  <style name="def:identifier"        foreground="fg"/>
  <style name="def:operator"          foreground="subtle"/>
  <style name="def:punctuation"       foreground="subtle"/>
  <style name="def:variable"          foreground="fg"/>

  <!-- Language-specific -->
  <style name="python:decorator"      foreground="cyan"   italic="true"/>
  <style name="python:f-string"       foreground="green"/>
  <style name="rust:lifetime"         foreground="yellow" italic="true"/>
  <style name="rust:macro"            foreground="cyan"/>
  <style name="rust:attribute"        foreground="muted"  italic="true"/>
  <style name="html:tag"              foreground="accent"/>
  <style name="html:attribute-name"   foreground="yellow"/>
  <style name="html:attribute-value"  foreground="green"/>
  <style name="css:property-name"     foreground="cyan"/>
  <style name="css:property-value"    foreground="green"/>
  <style name="css:selector"          foreground="accent"/>
  <style name="js:this"               foreground="red"/>
  <style name="js:arrow"              foreground="accent"/>
  <style name="xml:element-name"      foreground="accent"/>
  <style name="xml:attribute-name"    foreground="yellow"/>
  <style name="xml:attribute-value"   foreground="green"/>
  <style name="xml:comment"           foreground="muted"  italic="true"/>
  <style name="xml:cdata-delim"       foreground="cyan"/>
</style-scheme>
"#
    );

    if let Some(data_dir) = dirs::data_local_dir() {
        let styles_dir = data_dir.join("gtksourceview-5").join("styles");
        if let Err(e) = std::fs::create_dir_all(&styles_dir) {
            log::warn!("Could not create sourceview styles dir: {}", e);
            return;
        }
        let out_path = styles_dir.join("rui-theme.xml");
        if let Err(e) = std::fs::write(&out_path, &xml) {
            log::warn!("Could not write rui-theme.xml: {}", e);
        } else {
            log::info!("Wrote GtkSourceView scheme → {}", out_path.display());
        }
    }
}
