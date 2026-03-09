import os
from pathlib import Path

def print_tree(directory, prefix=""):
    # Semantic colors/icons
    ICONS = {
        "macos": "🍎",
        "windows": "🪟",
        "linux": "🐧",
        "python": "🐍",
        "node": "⬢ ",
        "arch": "🏔️",
        "oci": "🐳",
        "cli": "💻",
        "pkg": "📦",
        "deb": "📦",
        "rpm": "📦",
        "freebsd": "😈",
        "zip": "🤐",
        "whl": "🎡",
        "tgz": "🗜️",
        "tar.gz": "🗜️",
    }

    files = sorted(os.listdir(directory))
    for i, file in enumerate(files):
        path = os.path.join(directory, file)
        is_last = (i == len(files) - 1)
        connector = "└── " if is_last else "├── "
        
        # Determine icon
        icon = "📄"
        ext = "".join(Path(file).suffixes)
        for key in ICONS:
            if key in file.lower() or key in ext:
                icon = ICONS[key]
                break
        
        if os.path.isdir(path):
            print(f"{prefix}{connector}{icon} \033[1;34m{file}/\033[0m")
            new_prefix = prefix + ("    " if is_last else "│   ")
            print_tree(path, new_prefix)
        else:
            size_mb = os.path.getsize(path) / (1024 * 1024)
            print(f"{prefix}{connector}{icon} {file} \033[2m({size_mb:.1f} MB)\033[0m")

if __name__ == "__main__":
    print("\n\033[1;32m🚀 CURD Distribution Plane\033[0m")
    print("\033[2m" + "="*40 + "\033[0m")
    if os.path.exists("dist"):
        print_tree("dist")
    else:
        print("\033[31mError: ./dist directory not found. Run 'make dist' first.\033[0m")
    print("\033[2m" + "="*40 + "\033[0m\n")
