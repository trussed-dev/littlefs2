(function() {var implementors = {};
implementors["littlefs2"] = [{"text":"impl Freeze for Version","synthetic":true,"types":[]},{"text":"impl&lt;Storage&gt; Freeze for Allocation&lt;Storage&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;&lt;&lt;Storage as Storage&gt;::CACHE_SIZE as ArrayLength&lt;u8&gt;&gt;::ArrayType: Freeze,<br>&nbsp;&nbsp;&nbsp;&nbsp;&lt;&lt;Storage as Storage&gt;::LOOKAHEADWORDS_SIZE as ArrayLength&lt;u32&gt;&gt;::ArrayType: Freeze,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;'a, Storage&gt; !Freeze for Filesystem&lt;'a, Storage&gt;","synthetic":true,"types":[]},{"text":"impl Freeze for Metadata","synthetic":true,"types":[]},{"text":"impl&lt;S&gt; Freeze for Attribute&lt;S&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;&lt;&lt;S as Storage&gt;::ATTRBYTES_MAX as ArrayLength&lt;u8&gt;&gt;::ArrayType: Freeze,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;S&gt; Freeze for FileAllocation&lt;S&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;&lt;&lt;S as Storage&gt;::CACHE_SIZE as ArrayLength&lt;u8&gt;&gt;::ArrayType: Freeze,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;'a, 'b, S&gt; !Freeze for File&lt;'a, 'b, S&gt;","synthetic":true,"types":[]},{"text":"impl Freeze for OpenOptions","synthetic":true,"types":[]},{"text":"impl Freeze for DirEntry","synthetic":true,"types":[]},{"text":"impl Freeze for ReadDirAllocation","synthetic":true,"types":[]},{"text":"impl&lt;'a, 'b, S&gt; !Freeze for ReadDir&lt;'a, 'b, S&gt;","synthetic":true,"types":[]},{"text":"impl Freeze for FileType","synthetic":true,"types":[]},{"text":"impl Freeze for SeekFrom","synthetic":true,"types":[]},{"text":"impl Freeze for Error","synthetic":true,"types":[]},{"text":"impl Freeze for Path","synthetic":true,"types":[]},{"text":"impl Freeze for PathBuf","synthetic":true,"types":[]},{"text":"impl Freeze for Error","synthetic":true,"types":[]}];
if (window.register_implementors) {window.register_implementors(implementors);} else {window.pending_implementors = implementors;}})()