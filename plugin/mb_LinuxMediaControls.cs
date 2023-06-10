using System;
using System.IO;
using System.Runtime.InteropServices;
using System.Drawing;
using System.Windows.Forms;
using System.Collections.Generic;
using System.Text.RegularExpressions;
using System.Runtime.Serialization;
using System.Runtime.Serialization.Formatters.Soap;

namespace MusicBeePlugin
{
    public partial class Plugin
    {
        private PluginInfo about = new PluginInfo();

        private MusicBeeApiInterface mbApiInterface;
        private string ConfigDirectory {
            get { return mbApiInterface.Setting_GetPersistentStoragePath() + "\\linux-media-controls"; }
        }

        private string ConfigFile {
            get { return this.ConfigDirectory + "\\config.txt"; }
        }

        private Config config;
        private Communication communication;
        private ConfigPanel panel;

        private float volume = 1;

        public PluginInfo Initialise(IntPtr apiInterfacePtr)
        {
            mbApiInterface = new MusicBeeApiInterface();
            mbApiInterface.Initialise(apiInterfacePtr);
            about.PluginInfoVersion = PluginInfoVersion;
            about.Name = "Linux Media Controls";
            about.Description = "Provides media control support to linux under wine";
            about.Author = "ThatEpicBanana";
            about.TargetApplication = "";   //  the name of a Plugin Storage device or panel header for a dockable panel
            about.Type = PluginType.General;
            about.VersionMajor = 1;  // your plugin version
            about.VersionMinor = 0;
            about.Revision = 1;
            about.MinInterfaceVersion = MinInterfaceVersion;
            about.MinApiRevision = MinApiRevision;
            about.ReceiveNotifications = (ReceiveNotificationFlags.PlayerEvents | ReceiveNotificationFlags.TagEvents);
            about.ConfigurationPanelHeight = 20;   // height in pixels that musicbee should reserve in a panel for config settings. When set, a handle to an empty panel will be passed to the Configure function

            Directory.CreateDirectory(this.ConfigDirectory);
            this.config = this.getConfig();
            this.panel = new ConfigPanel(this.config);

            this.communication = new Communication(this.config, this.mbApiInterface, this);

            this.CreateFileStructure();

            return about;
        }

        public bool Configure(IntPtr panelHandle)
        {
            // panelHandle will only be set if you set about.ConfigurationPanelHeight to a non-zero value
            // keep in mind the panel width is scaled according to the font the user has selected
            // if about.ConfigurationPanelHeight is set to 0, you can display your own popup window
            if (panelHandle != IntPtr.Zero)
            {
                this.panel.attach(panelHandle);
            }
            return false;
        }

        // called by MusicBee when the user clicks Apply or Save in the MusicBee Preferences screen.
        // its up to you to figure out whether anything has changed and needs updating
        public void SaveSettings()
        {
            this.config = this.panel.update();
            this.saveConfig(this.config);
        }

        // MusicBee is closing the plugin (plugin is being disabled by user or MusicBee is shutting down)
        public void Close(PluginCloseReason reason)
        {
            this.Deactivate();
        }

        // uninstall this plugin - clean up any persisted files
        public void Uninstall()
        {
            Directory.Delete(this.ConfigDirectory);
        }

        // receive event notifications from MusicBee
        public void ReceiveNotification(string sourceFileUrl, NotificationType type)
        {
            switch (type)
            {
                case NotificationType.PluginStartup:
                    this.UpdateVolume();
                    this.Update();
                    this.Activate();
                    break;
                case NotificationType.TrackChanged:
                // TODO: this doesn't do the right thing, find some other event for when the current file's tags are changed
                /* case NotificationType.TagsChanged: // dunno if this does anything but might as well */
                    this.UpdateMetaData();
                    break;
                case NotificationType.PlayStateChanged:
                    this.UpdatePlayback();
                    break;
                case NotificationType.VolumeLevelChanged:

                    // HACK: the handler communicates to plugin by changing volume.
                    // If a file in the communication directory has a new action, 
                    //   then it'll perform that action and reset the volume.
                    // If it doesn't, then it'll do nothing (which is very 
                    //   important to not start an infinite loop)
                    // Find something better if possible.
                    this.RecieveCommand();

                    break;
            }
        }

        private void RecieveCommand()
        {
            if (this.communication.handleAction()) {
                this.ResetVolume();
            } else {
                this.UpdateVolume();
            }
        }

        private void UpdateVolume() { this.volume = mbApiInterface.Player_GetVolume(); }
        private void ResetVolume() { mbApiInterface.Player_SetVolume(this.volume); }

        private void CreateFileStructure()
        {
            Directory.CreateDirectory(this.config.rootDirectory);
            File.Create(this.config.rootDirectory + Communication.activatedFile).Close();
            File.Create(this.config.rootDirectory + Communication.playbackFile).Close();
            File.Create(this.config.rootDirectory + Communication.metadataFile).Close();
            File.Create(this.config.rootDirectory + Communication.actionFile).Close();
        }

        private void Activate()
        {
            File.WriteAllText(this.config.rootDirectory + Communication.activatedFile, "true");
        }

        private void Deactivate()
        {
            File.WriteAllText(this.config.rootDirectory + Communication.activatedFile, "false");
        }

        private void Update()
        {
            this.UpdatePlayback();
            this.UpdateMetaData();
        }

        private void UpdatePlayback() 
        {
            string state = null;

            switch (mbApiInterface.Player_GetPlayState())
            {
                case PlayState.Paused:
                    state = "paused";
                    break;
                case PlayState.Playing:
                    state = "playing";
                    break;
                case PlayState.Stopped:
                    state = "stopped";
                    break;
                default:
                    state = "loading";
                    break;
            }

            int position = mbApiInterface.Player_GetPosition();

            this.communication.write(Communication.playbackFile,
                state + "\n" +
                position
            );
        }

        private void UpdateMetaData() 
        {
            string title = mbApiInterface.NowPlaying_GetFileTag(MetaDataType.TrackTitle);
            string album = mbApiInterface.NowPlaying_GetFileTag(MetaDataType.Album);
            string artist = mbApiInterface.NowPlaying_GetFileTag(MetaDataType.Artist);
            string cover = mbApiInterface.NowPlaying_GetArtworkUrl();
            int duration = mbApiInterface.NowPlaying_GetDuration();

            this.communication.write(Communication.metadataFile,
                title + "\n" +
                album + "\n" +
                artist + "\n" +
                cover + "\n" +
                duration
            );
        }

        private Config getConfig() {
            Config val;

            if(File.Exists(this.ConfigFile)) {
                val = Config.deserialize(File.ReadAllText(this.ConfigFile));
            } else {
                val = Config.def();
                this.saveConfig(val);
            }

            return val;
        }

        private void saveConfig(Config config) {
            File.WriteAllText(this.ConfigFile, config.serialize());
        }

        class Communication {
            public const string metadataFile = "metadata";
            public const string playbackFile = "playback";
            public const string activatedFile = "plugin-activated";
            public const string actionFile = "action";

            private Config config;
            private MusicBeeApiInterface mbApiInterface;
            private Plugin plugin;

            public Communication(Config config, MusicBeeApiInterface mbApiInterface, Plugin plugin) {
                this.config = config;
                this.mbApiInterface = mbApiInterface;
                this.plugin = plugin;
            }

            public void write(string file, string text) {
                File.WriteAllText(this.config.rootDirectory + file, text); 
            }

            public string get(string file) {
                return File.ReadAllText(this.config.rootDirectory + file);
            }

            // returns if an action was handled
            public bool handleAction() {
                string action = get(Communication.actionFile);
                if(string.IsNullOrWhiteSpace(action)) return false;

                string[] args = action.Trim().Split();

                // no-arg commands
                if(args[0] == "play")
                    mbApiInterface.Player_PlayPause();

                bool volumeChanged = false;

                // single argument commands
                // TODO: some kind of logging
                if(args.Length > 1) {
                    switch (args[0])
                    {
                        case "shuffle":
                            this.updateShuffle(args[1]);
                            break;
                        case "repeat":
                            this.updateRepeat(args[1]);
                            break;
                        case "seek":
                            this.seek(args[1]);
                            break;
                        case "position":
                            this.setPosition(args[1]);
                            this.plugin.Update();
                            break;
                        case "volume":
                            this.setVolume(args[1]);
                            volumeChanged = true;
                            break;
                    }
                }

                write(Communication.actionFile, "");
                return !volumeChanged;
            }

            private void updateShuffle(string arg) 
            {
                switch (arg)
                {
                    case "on":
                    case "true":
                        mbApiInterface.Player_SetShuffle(true);
                        return;
                    case "toggle":
                        mbApiInterface.Player_SetShuffle(!mbApiInterface.Player_GetShuffle());
                        return;
                    default: // "off" / "false"
                        mbApiInterface.Player_SetShuffle(false);
                        return;
                }
            }
            
            private void updateRepeat(string arg)
            {
                switch (arg)
                {
                    case "one":
                        mbApiInterface.Player_SetRepeat(RepeatMode.One);
                        return;
                    case "none":
                        mbApiInterface.Player_SetRepeat(RepeatMode.None);
                        return;
                    default: // "all"
                        mbApiInterface.Player_SetRepeat(RepeatMode.All);
                        return;
                }
            }

            private void parseIntAnd(string val, Action<int> callback) 
            {
                try {
                    callback(Int32.Parse(val));
                // TODO: log
                } catch (FormatException) {}
            }

            private void seek(string str)
            {
                parseIntAnd(str, amt => {
                    mbApiInterface.Player_SetPosition(mbApiInterface.Player_GetPosition() + amt);
                });
            }

            private void setPosition(string str) 
            {
                parseIntAnd(str, pos => {
                    mbApiInterface.Player_SetPosition(pos);
                });
            }

            private void setVolume(string str)
            {
                parseIntAnd(str, to => {
                    mbApiInterface.Player_SetVolume((float) to / 100);
                });
            }
        }
    }

    [Serializable()]
    class Config {
        private static Regex file_regex = new Regex("^([a-zA-Z]\\:)(\\\\[^\\\\/:*?<>\"|]*(?<![ ]))*(\\.[a-zA-Z]{2,6})$", RegexOptions.Compiled);

        public string rootDirectory;

        public static Config def() {
            return new Config() { rootDirectory = "Z:\\\\tmp\\musicbee-mediakeys\\" };
        }

        // checks if some configuration is valid
        // this may be able to fix some issues if invalid
        // returns true if the configuration is valid (including if it has been fixed), and false if it isn't
        public bool validate() {
            return validateRoot();
        }

        private bool validateRoot() {
            string root = this.rootDirectory;

            // replace \ with /
            root = root.Replace('/', '\\');
            // if the root starts with /, assume the user meant a linux location
            if(root.StartsWith("\\")) 
                root = "Z:" + root;
            // add a \ at the end if it isn't present 
            if(!root.EndsWith("\\"))
                root = root + '\\';
            // replace c:\ with c:\\
            if(!root.Contains(":\\\\"))
                root = root.Replace(":\\", ":\\\\");

            // check if it's valid
            if(!ReferenceEquals(new FileInfo(root), null)) {
                this.rootDirectory = root;
                return true;
            } else return false;
        }

        // I originally used xml for this, but 
        // the serializer doesn't work with backslashes well
        public string serialize() {
            return this.rootDirectory;
        }

        public static Config deserialize(string val) {
            return new Config() {
                rootDirectory = val,
            };
        }
    }

    class ConfigPanel {
        private Config config;

        private Label rootLabel;
        private TextBox rootBox;

        public ConfigPanel(Config config) {
            this.config = config;
        }

        public Config update() {
            Config config = new Config() {
                rootDirectory = this.rootBox.Text
            };

            // fallback to old config if it's broken
            if(config.validate()) {
                this.config = config;
                this.updatePanel();
                return this.config;
            } else {
                return this.config;
            }
        }

        public void attach(IntPtr panelHandle) {
            Panel panel = (Panel) Panel.FromHandle(panelHandle);

            this.rootLabel = new Label() {
                Text = "Root Directory: ",
                AutoSize = true,
                Location = new Point(0, 0)
            };

            this.rootBox = new TextBox() {
                Bounds = new Rectangle(this.rootLabel.Width, 0, 200, this.rootLabel.Height)
            };

            this.updatePanel();

            panel.Controls.AddRange(new Control[] { this.rootLabel, this.rootBox });
        }

        public void updatePanel() {
            this.rootBox.Text = this.config.rootDirectory;
        }
    }
}
