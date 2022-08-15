<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.0 Transitional//EN" "http://www.w3.org/TR/xhtml1/DTD/xhtml1-transitional.dtd">
<html xmlns="http://www.w3.org/1999/xhtml">
    <head>
        <meta http-equiv="X-UA-Compatible" content="IE=Edge" />
        <meta http-equiv="Content-Type" content="text/html; charset=utf-8" />
        <meta http-equiv="Pragma" content="no-cache" />
        <meta http-equiv="Expires" content="-1" />
        <link rel="shortcut icon" href="images/favicon.png" />
        <link rel="icon" href="images/favicon.png" />
        <title>PIKPAK网盘 WebDAV</title>
        <link rel="stylesheet" type="text/css" href="index_style.css" />
        <link rel="stylesheet" type="text/css" href="form_style.css" />
        <link rel="stylesheet" type="text/css" href="usp_style.css" />
        <link rel="stylesheet" type="text/css" href="css/element.css" />
        <script type="text/javascript" src="/js/jquery.js"></script>
        <script src="/state.js"></script>
        <script src="/help.js"></script>
    </head>
    <body>
        <div id="TopBanner"></div>
        <div id="Loading" class="popup_bg"></div>
        <table class="content" align="center" cellpadding="0" cellspacing="0">
            <tbody>
                <tr>
                    <td width="17">&nbsp;</td>
                    <td valign="top" width="202">
                        <div id="mainMenu"></div>
                        <div id="subMenu"></div>
                    </td>
                    <td valign="top">
                        <div id="tabMenu" class="submenuBlock"></div>
                        <!--=====Beginning of Main Content=====-->
                        <table width="98%" border="0" align="left" cellpadding="0" cellspacing="0" style="display: block;">
                            <tbody>
                                <tr>
                                    <td align="left" valign="top">
                                        <div>
                                            <table width="760px" border="0" cellpadding="5" cellspacing="0" bordercolor="#6b8fa3" class="FormTitle" id="FormTitle">
                                                <tbody>
                                                    <tr>
                                                        <td bgcolor="#4D595D" colspan="3" valign="top">
                                                            <div>&nbsp;</div>
                                                            <div style="float:left;" class="formfonttitle" style="padding-top: 12px">PIKPAK网盘WebDAV - 设置</div>
                                                            <div style="float:right; width:15px; height:25px;margin-top:10px"><img id="return_btn" onclick="reload_Soft_Center();" align="right" style="cursor:pointer;position:absolute;margin-left:-30px;margin-top:-25px;" title="返回软件中心" src="/images/backprev.png" onMouseOver="this.src='/images/backprevclick.png'" onMouseOut="this.src='/images/backprev.png'"></img></div>
                                                            <div style="margin-left:5px;margin-top:10px;margin-bottom:10px"><img src="/images/New_ui/export/line_export.png"></div>
                                                            <div class="SimpleNote" id="head_illustrate">
                                                                <p>PIKPAK网盘  可以登录<a href="https://mypikpak.com/" target="_blank">PIKPAK网盘网页版</a></p>
                                                            </div>
                                                            <table style="margin:20px 0px 0px 0px;" width="100%" border="1" align="center" cellpadding="4" cellspacing="0" bordercolor="#6b8fa3" class="FormTable">
                                                                <thead>
                                                                    <tr>
                                                                        <td colspan="2">PIKPAK网盘 WebDAV - 设置面板</td>
                                                                    </tr>
                                                                </thead>
                                                                <tbody>
                                                                    <tr id="switch_tr">
                                                                        <th> <label>开启PIKPAK网盘 WebDAV</label> </th>
                                                                        <td colspan="2">
                                                                            <div class="switch_field" style="display:table-cell">
                                                                                <label for="switch">
                                                                                    <input id="switch" class="switch" type="checkbox" style="display: none;" />
                                                                                    <div class="switch_container">
                                                                                        <div class="switch_bar"></div>
                                                                                        <div class="switch_circle transition_style">
                                                                                            <div></div>
                                                                                        </div>
                                                                                    </div>
                                                                                </label>
                                                                            </div>
                                                                            <div id="koolproxy_install_show" style="padding-top:5px;margin-left:80px;margin-top:-30px;float: left;"></div>
                                                                        </td>
                                                                    </tr>
                                                                    

                                                                    <tr id="user_tr">
                                                                        <th>PikPak用户名【目前只支持邮箱】</th>
                                                                        <td> <input type="text" id="pikpakwebdav_user" value="<% dbus_get_def("pikpakwebdav_user", ""); %>" class="input_ss_table"></td>
                                                                    </tr>

                                                                    <tr id="password_tr">
                                                                        <th>PikPak密码</th>
                                                                        <td> <input type="text" id="pikpakwebdav_password" value="<% dbus_get_def("pikpakwebdav_password", ""); %>" class="input_ss_table"></td>
                                                                    </tr>
                                                                    <tr id="proxy_url_tr">
                                                                        <th>代理网址</th>
                                                                        <td> <input type="text" id="pikpakwebdav_proxy_url" value="<% dbus_get_def("pikpakwebdav_proxy_url", ""); %>" class="input_ss_table"></td>
                                                                    </tr>

                                                                    <tr id="root_tr">
                                                                        <th>根目录</th>
                                                                        <td> <input type="text" id="pikpakwebdav_root" value="<% dbus_get_def("pikpakwebdav_root", "/"); %>" class="input_ss_table"></td>
                                                                    </tr>
                                                                    <tr id="port_tr">
                                                                        <th>监听端口</th>
                                                                        <td><input type="text" id="pikpakwebdav_port" value="<% dbus_get_def("pikpakwebdav_port", "9867"); %>" class="input_ss_table"></td>
                                                                    </tr>
                                                                    <tr id="auth_user_tr">
                                                                        <th>用户名</th>
                                                                        <td><input type="text" id="pikpakwebdav_auth_user" value="<% dbus_get_def("pikpakwebdav_auth_user", ""); %>" class="input_ss_table"></td>
                                                                    </tr>
                                                                    <tr id="auth_password_tr">
                                                                        <th>密码</th>
                                                                        <td><input type="text" id="pikpakwebdav_auth_password" value="<% dbus_get_def("pikpakwebdav_auth_password", ""); %>" class="input_ss_table"></td>
                                                                    </tr>
                                                                    <tr id="read_buffer_size_tr">
                                                                        <th>下载缓冲大小(bytes)</th>
                                                                        <td><input type="text" id="pikpakwebdav_read_buffer_size" value="<% dbus_get_def("pikpakwebdav_read_buffer_size", "10485760"); %>" class="input_ss_table"></td>
                                                                    </tr>
                                                                    <tr id="cache_size_tr">
                                                                        <th>目录缓存大小</th>
                                                                        <td><input type="text" id="pikpakwebdav_cache_size" value="<% dbus_get_def("pikpakwebdav_cache_size", "1000"); %>" class="input_ss_table"></td>
                                                                    </tr>
                                                                </tbody>
                                                            </table>
                                                            <div class="apply_gen">
                                                                <input class="button_gen" type="button" value="提交" />
                                                            </div>
                                                            <div style="margin-left:5px;margin-top:10px;margin-bottom:10px">
                                                                <img src="/images/New_ui/export/line_export.png" />
                                                            </div>
                                                            <div class="KoolshareBottom" style="margin-top:540px;">
                                                                论坛技术支持：
                                                                <a href="http://www.koolshare.cn" target="_blank"> <i><u>www.koolshare.cn</u></i> </a><br/>
                                                                Github项目：
                                                                <a href="https://github.com/koolshare/koolshare.github.io/tree/acelan_softcenter_ui" target="_blank"> <i><u>github.com/koolshare</u></i> </a><br />
                                                            </div>
                                                        </td>
                                                    </tr>
                                                </tbody>
                                            </table>
                                        </div>
                                    </td>
                                </tr>
                            </tbody>
                        </table>
                        <!--=====end of Main Content=====-->
                    </td>
                </tr>
            </tbody>
        </table>
        <div id="footer"></div>
        <script>
            $(function () {
                show_menu(menu_hook);
                var enable = "<% dbus_get_def("pikpakwebdav_enable", "0"); %>";
                $('#switch').prop('checked', enable === "1");
                buildswitch();
                update_visibility();
                var posting = false;
                var inputs = ['user','password','proxy_url' 'port', 'auth_user', 'auth_password', 'read_buffer_size', 'cache_size', 'root'];
                $('.button_gen').click(function () {
                    if(posting) return;
                    posting = true; // save
            		var data = {
            			pikpakwebdav_enable: $('#switch').prop('checked') | 0,
            			action_mode: ' Refresh ',
            			current_page: 'Module_pikpakwebdav.asp',
            			next_page: 'Module_pikpakwebdav.asp',
            			SystemCmd: 'pikpakwebdav_config.sh'
            		};
            		for(var i = 0; i< inputs.length; i++) {
            			var key = 'pikpakwebdav_' + inputs[i];
            			data[key] = $('#'+key).val()
            		}
                    $.ajax({
                        type: 'POST',
                        url: 'applydb.cgi?p=pikpakwebdav_',
                        data: $.param(data)
                    }).then(function () {
                        posting = false;
                        alert('配置保存成功...');
                    }, function () {
                        posting = false;
                       alert('配置保存失败!');
                    })
                })
            })

            function menu_hook(title, tab) {
                tabtitle[tabtitle.length -1] = new Array("", "PIKPAK网盘 WebDAV");
                tablink[tablink.length -1] = new Array("", "Module_pikpakwebdav.asp");
            }

            function reload_Soft_Center(){
                location.href = "/Main_Soft_center.asp";
            }

            function buildswitch(){
            	$("#switch").click(
            	function(){
            		update_visibility();
            	});
            }

            function update_visibility(){
                if (document.getElementById('switch').checked) {
                    document.getElementById("user_tr").style.display = "";
                    document.getElementById("password_tr").style.display = "";
                    document.getElementById("proxy_url_tr").style.display = "";
                    document.getElementById("root_tr").style.display = "";
                    document.getElementById("port_tr").style.display = "";
                    document.getElementById("auth_user_tr").style.display = "";
                    document.getElementById("auth_password_tr").style.display = "";
                    document.getElementById("read_buffer_size_tr").style.display = "";
                    document.getElementById("cache_size_tr").style.display = "";

                } else {
                    document.getElementById("user_tr").style.display = "none";
                    document.getElementById("password_tr").style.display = "none";
                    document.getElementById("proxy_url_tr").style.display = "none";
                    document.getElementById("root_tr").style.display = "none";
                    document.getElementById("port_tr").style.display = "none";
                    document.getElementById("auth_user_tr").style.display = "none";
                    document.getElementById("auth_password_tr").style.display = "none";
                    document.getElementById("read_buffer_size_tr").style.display = "none";
                    document.getElementById("cache_size_tr").style.display = "none";
                }
            }
        </script>
    </body>