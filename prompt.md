cargo install --path crates/cli --force
  Installing neojoplin-cli v0.1.4 (/home/konrad/gallery/neojoplin/crates/cli)
    Updating crates.io index
     Locking 370 packages to latest compatible versions
      Adding coolor v0.5.0 (available: v0.5.1)
      Adding crossterm v0.28.1 (available: v0.29.0)
      Adding dialoguer v0.11.0 (available: v0.12.0)
      Adding dirs v5.0.1 (available: v6.0.0)
      Adding generic-array v0.14.7 (available: v0.14.9)
      Adding hmac v0.12.1 (available: v0.13.0)
      Adding minimad v0.11.0 (available: v0.14.0)
      Adding pbkdf2 v0.12.2 (available: v0.13.0)
      Adding quick-xml v0.37.5 (available: v0.39.2)
      Adding rand v0.8.6 (available: v0.10.1)
      Adding ratatui v0.28.1 (available: v0.30.0)
      Adding reqwest v0.12.28 (available: v0.13.2)
      Adding sha2 v0.10.9 (available: v0.11.0)
      Adding termimad v0.22.0 (available: v0.34.1)
      Adding thiserror v1.0.69 (available: v2.0.18)
      Adding toml v0.8.23 (available: v1.1.2+spec-1.1.0)

- Couldn't we just update these packages? I think, outdated packages are always bad.
- Use ratatui-interact. Download it and assess where it can be used. I just committed changes, because on my viewport. There was a problem, that not all contents of the popup dialogue for deletion confirmation was shown, I think a higher-level package based on ratatui will be helpful so that you don't need to handle the dialogues yourself. Read the documentation and then implement the new popups for all dialogues, so that they don't depend on window percentages any more. On small view ports, with this naive technique, you wast a lot of space. 
- In the auto-sync tab of the settings dialogue, it is not clear, which setting is currently turned on. The interface suggests, that each time, the cursor changes, the setting is changed. Is that true? Think about if that is the right interface, maybe you find a different solution.
- In the status tab of the settings dialogue, there should be and information on the current auto-sync setting and when the next auto-sync is scheduled.
- The syncing is my major concern. In the sync targets section of the settings, I want all the targets available that are also available in the joplin command line application. I think, there is only one sync target per type or is it possible to have the application sync to multiple targets? Look that up in the `~/gallery/kjoplin/joplin` code folder. If there is the possibility to sync to WebDAV and to other targets at the same time, I see no problem to have multiple WebDAV targets. If it is only one target that should be active, the interface should clearly tell, which one is active. It might suffice to tell the user to select the target that is selected will be the only active. This is only a contingency plan. When the joplin command line application supports multiple targets, there is no need to choose. Then, you should implement the functionality to sync to multiple targets, too. The architecture for this, is my major concern. There should be crates which have all there clearly separated functionality so they don't get bloated and can be maintained independently. So, think about how to integrate the multiple source syncing functionality (if that is even necessary and not already done).
- In the settings dialogue, the information in the encryption tab is a little sparse. There are master keys that are used to encrypt and decrypt notes. For this to work, the master keys need to be paired with a master password. One master password can be used with multiple master keys. Is that about right? This more detailed information should be in the help section of neojoplin (in a separate tab), but the encryption tab in the settings should contain information on the master keys that are provided. In the electron joplin desktop application (see in `~/gallery/kjoplin/joplin`), you find some functionality for that, that you should port to neojoplin.
