/** 可连接打印机信息 */
export interface PrinterItem {
  /** 打印机显示名称（共享名） */
  name: string
  /** 完整 SMB 共享路径 */
  share_path: string
  /** 驱动名称 */
  driver_name: string
  /** 设备状态 */
  status: string
}

/** 本地已连接打印机信息 */
export interface LocalPrinterItem {
  /** 打印机名称（UNC 路径） */
  name: string
  /** 端口名称 */
  port_name: string
  /** 驱动名称 */
  driver_name: string
  /** 是否为默认打印机 */
  is_default: boolean
  /** 设备状态 */
  status: string
}

/** 状态指示 */
export type StatusState = 'ok' | 'error' | 'checking'

/** 应用配置（打印服务器连接信息） */
export interface AppConfig {
  /** 打印服务器地址（IP 或主机名） */
  server_addr: string
  /** SMB 凭据账号 */
  username: string
  /** SMB 凭据密码 */
  password: string
}
